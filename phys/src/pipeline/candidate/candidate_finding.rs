use crate::constants::{MAX_COMBINATIONS_PER_SYMBOL_AND_AL, MAX_ERRORS_AND_FREE_AL_1, TRY_FITTING_AL_FIRST};
use crate::grid::grid_info::{SUBCARRIERS_PER_PRB, SUBFRAMES_PER_FRAME};
use crate::grid::numerology::Numerology;
use crate::metrics::{CandidatePipelineMetrics, TimeMeasurement};
use crate::pipeline::candidate::active_group_detection::{ActiveGroupDetectionError, ActiveGroupFrame, ActiveGroups, PDCCH_SUBCARRIERS_PER_PRB};
use crate::pipeline::candidate::combinations::Combinations;
use crate::pipeline::candidate::config::coreset::CoresetCandidates;
use crate::pipeline::candidate::config::extra_candidates_config::Candidate;
use crate::pipeline::candidate::dci_classification::{classify_dci, DciClassification};
use crate::pipeline::candidate::grouped_combinations::{grouped_combinations_2, grouped_combinations_3, grouped_combinations_6};
use crate::pipeline::candidate::sequences::Sequences;
use crate::pipeline::pipeline::{PipelineLogEntry, PipelineLogLevel};
use pdcch::decoder;
use pdcch::decoder::{pdcch_decode_shared_slice, pdcch_decode_slice, BITS_AL_1, BITS_AL_16};
use rayon::prelude::{IntoParallelIterator, ParallelIterator, IndexedParallelIterator};
use serde::Serialize;
use std::cmp::{min, Ordering};
use num_complex::Complex32;
use rustfft::num_traits::{Signed, Zero};
use three_gpp::polar::DCI_CRC_BITS;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot;
use crate::equalization::channels::pdcch::get_dmrs_ref_pdcch_var;

#[allow(unused)]
const CF_MODULE: &str = "CANDIDATE_FINDER";

#[derive(Debug, Serialize)]
pub struct DCI {
    pub sfn: usize,
    pub symbol: usize,
    pub n_sym: usize,
    pub al: usize,
    pub time_ns: u128,
    pub lowest_group: usize,
    pub scs: u64,
    pub errors: usize,
    pub free: usize,
    pub rnti: u16,
    pub n_id: u16,
    pub shared: bool,
    pub classification: DciClassification,
    pub data: Vec<u8>,
}

pub struct CandidateFindingState {
    pub(crate) bwps: Vec<BWPCandidateFindingState>,
}

pub struct BWPCandidateFindingState {
    pub offset_to_carrier: u16,
    pub scs: u64,
    pub extended: bool,
    pub slots_per_subframe: usize,
    pub symbols_per_slot: usize,
    pub bandwidth_re: u16,
    pub(crate) coreset_candidates: Vec<CoresetCandidates>,
    pub(crate) extra_candidates: Vec<Candidate>,
}

impl CandidateFindingState {
    fn iter_candidates(&self, bwp_id: usize, sfn: usize, slot_num: usize) -> impl Iterator<Item = (bool, usize, &Candidate)>
    {
        let state = &self.bwps[bwp_id];
        state.coreset_candidates.iter().enumerate().flat_map(move |(j, x)| x.as_iter(sfn, slot_num, j))
            .chain(
                state.extra_candidates.iter().enumerate().map(|(i, x)| (false, i, x))
            )
    }

    fn get_candidate_mut(&mut self, bwp_id: usize, coreset: bool, ind: usize) -> Option<&mut Candidate> {
        let state = &mut self.bwps[bwp_id];
        if coreset {
            let j = ind >> 16;
            let i = ind & 0xFFFF;
            state.coreset_candidates.get_mut(j)?.candidates.get_mut(i)
        } else {
            state.extra_candidates.get_mut(ind)
        }
    }

    fn push_extra_candidates(&mut self, bwp_id: usize, candidates: &mut Vec<Candidate>) {
        let state = &mut self.bwps[bwp_id];
        // Merge candidate sets
        'outer:
        for candidate in candidates.drain(..) {
            for existing in state.extra_candidates.iter_mut() {
                if existing.shared == candidate.shared && existing.symbol == candidate.symbol && existing.n_sym == candidate.n_sym && existing.groups == candidate.groups {
                    // We have an equal existing entry
                    for dci_len in candidate.known_dci_lens {
                        if !existing.known_dci_lens.contains(&dci_len) {
                            existing.known_dci_lens.push(dci_len);
                        }
                    }
                    continue 'outer
                }
            }
            // No match with existing candidates found
            state.extra_candidates.push(candidate);
        }
    }

    pub fn num_bwps(&self) -> usize {
        self.bwps.len()
    }

    pub fn get_extra_candidates(&self, bwp_id: usize) -> &[Candidate] {
        self.bwps[bwp_id].extra_candidates.as_slice()
    }

    pub fn get_coreset_candidates(&self, bwp_id: usize) -> &[CoresetCandidates] {
        self.bwps[bwp_id].coreset_candidates.as_slice()
    }
}

pub async fn candidate_finding<const EXPAND_CANDIDATES: bool>(mut recv: Receiver<Result<ActiveGroupFrame, ActiveGroupDetectionError>>, tx: Sender<Result<DCI, ActiveGroupDetectionError>>, metrics_tx: Sender<CandidatePipelineMetrics>, state_tx: oneshot::Sender<CandidateFindingState>, mut state: CandidateFindingState, log: Sender<PipelineLogEntry>, search_al: Vec<usize>, max_frames: Option<usize>, max_candidate_finding_frames: Option<usize>, cf_fast_mode: bool, cf_super_fast_mode: bool, cf_error_correction: bool, sn_error_correction: bool, sn_expand: bool, use_dmrs: bool) {
    tokio::spawn(async move {
        // Cache some buffers
        let mut buffer = vec![vec![0f32; BITS_AL_16]; state.bwps.iter().map(|bwp| bwp.slots_per_subframe).max().unwrap_or(0) * SUBFRAMES_PER_FRAME];
        let mut found_candidates_buffer = Vec::with_capacity(1024);
        let mut dci_buffer = Vec::with_capacity(1024);
        let mut dci_sizes_buffer = Vec::with_capacity(128);
        // Collect a list of already known dci sizes
        state.bwps.iter().for_each(|bwp| bwp.coreset_candidates.iter().for_each(|c| c.candidates.iter().for_each(|c| c.known_dci_lens.iter().for_each(|c| {
            if !dci_sizes_buffer.contains(c) {
                dci_sizes_buffer.push(*c);
            }
        } ))));
        state.bwps.iter().for_each(|bwp| bwp.extra_candidates.iter().for_each(|c| c.known_dci_lens.iter().for_each(|c| {
            if !dci_sizes_buffer.contains(c) {
                dci_sizes_buffer.push(*c);
            }
        } )));
        dci_sizes_buffer.sort();

        let mut global_frame_counter = 0;

        loop {
            match recv.recv().await {
                Some(r) => match r {
                    Ok(frame) => {
                        if let Some(mcff) = max_candidate_finding_frames && mcff <= global_frame_counter {
                            if mcff == global_frame_counter {
                                let _ = log.send(PipelineLogEntry::of(PipelineLogLevel::Trace, CF_MODULE, format!("Disabling candidate finding at frame {} (sfn = {})", mcff, frame.sfn))).await;
                            }
                            if process_frame::<false>(frame, &mut state, &tx, &metrics_tx, &log, search_al.as_slice(), &mut buffer, global_frame_counter, &mut found_candidates_buffer, &mut dci_buffer, &mut dci_sizes_buffer, cf_fast_mode, cf_super_fast_mode, cf_error_correction, sn_error_correction, sn_expand, use_dmrs).await {
                                // True means eof
                                let _ = state_tx.send(state);
                                return;
                            }
                        } else {
                            if process_frame::<EXPAND_CANDIDATES>(frame, &mut state, &tx, &metrics_tx, &log, search_al.as_slice(), &mut buffer, global_frame_counter, &mut found_candidates_buffer, &mut dci_buffer, &mut dci_sizes_buffer, cf_fast_mode, cf_super_fast_mode, cf_error_correction, sn_error_correction, sn_expand, use_dmrs).await {
                                // True means eof
                                let _ = state_tx.send(state);
                                return;
                            }
                        }

                        global_frame_counter += 1;
                        // Check whether we processed enough frames
                        if let Some(max_frames) = &max_frames && global_frame_counter >= *max_frames {
                            let _ = state_tx.send(state);
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        let _ = state_tx.send(state);
                        return;
                    }
                }
                None => {
                    let _ = state_tx.send(state);
                    return;
                }
            }
        }
    });
}

pub async fn process_frame<const EXPAND_CANDIDATES: bool>(mut frame: ActiveGroupFrame, state: &mut CandidateFindingState, tx: &Sender<Result<DCI, ActiveGroupDetectionError>>, metrics_tx: &Sender<CandidatePipelineMetrics>, #[allow(
    unused
)] log: &Sender<PipelineLogEntry>, search_al: &[usize], buffer: &mut Vec<Vec<f32>>, global_frame_counter: usize, found_candidates_buffer: &mut Vec<(usize, usize, Vec<usize>, usize, bool)>, dci_buffer: &mut Vec<DCI>, dci_sizes_buffer: &mut Vec<usize>, cf_fast_mode: bool, cf_super_fast_mode: bool, cf_error_correction: bool, sn_error_correction: bool, sn_expand: bool, use_dmrs: bool) -> bool {
    // Step 1: Attempt to read all candidates already known and on success remove from active groups
    let time = TimeMeasurement::start();
    for bwp_id in 0..frame.bwps.len() {
        let bwp = &frame.bwps[bwp_id];
        let scs = bwp.scs;
        let ext = bwp.extended;
        let offset_to_carrier = bwp.offset_to_carrier;
        let sym_per_slot = state.bwps[bwp_id].symbols_per_slot;
        let slots_per_subframe = state.bwps[bwp_id].slots_per_subframe;
        let dci_sizes_buffer = dci_sizes_buffer.as_slice();
        let d = (0..slots_per_subframe * SUBFRAMES_PER_FRAME).into_par_iter().zip(&mut *buffer).map(|(slot, buffer)| {
            state.iter_candidates(bwp_id, frame.sfn, slot).map(move |(coreset, writeback_ind, candidate)| {
                let f = if !sn_error_correction || (MAX_ERRORS_AND_FREE_AL_1 == 0 && candidate.n_sym * candidate.groups.len() == 6) {
                    // Disable error correction
                    try_decode_candidate::<true, false>
                } else {
                    // Enable error correction
                    try_decode_candidate::<true, true>
                };
                let candidate_sym = candidate.symbol + slot * sym_per_slot;
                if let Some(dci) = f(global_frame_counter, frame.sfn, scs, ext, candidate_sym, candidate.n_sym, candidate.groups.as_slice(), bwp, buffer.as_mut_slice(), frame.cell_id, if sn_expand && EXPAND_CANDIDATES {
                    // If we are expanding while sniffing candidates we search for all possible DCI sizes
                    &[]
                } else if EXPAND_CANDIDATES {
                    // If we are expanding candidates and sniffing without candidate expansion, we at least check for all different DCI sizes already known
                    dci_sizes_buffer
                } else {
                    // Else we only check known sizes for this candidate
                    candidate.known_dci_lens.as_slice()
                }, candidate.shared, use_dmrs) {
                    if EXPAND_CANDIDATES {
                        let len = dci.data.len();
                        (Some(dci), Some((coreset, writeback_ind, len)), Some(candidate_sym))
                    } else {
                        (Some(dci), None, None)
                    }
                } else {
                    (None, None, None)
                }
            })
        }).flatten_iter().collect::<Vec<_>>();
        for d in d {
            if EXPAND_CANDIDATES && let (Some(dci), Some((coreset, writeback_ind, len)), Some(candidate_sym)) = d {
                let candidate = state.get_candidate_mut(bwp_id, coreset, writeback_ind).expect("Candidate vanished. This is unexpected.");

                // Make sure candidate size is listed in known dci lengths
                if !candidate.known_dci_lens.contains(&len) {
                    candidate.known_dci_lens.push(len);
                }
                // Remove candidate from active groups
                remove_candidate_from_active(scs, ext, offset_to_carrier, candidate_sym, candidate.n_sym, candidate.groups.as_slice(), &mut frame.bwps);
                // Push DCI
                dci_buffer.push(dci);
            } else if let (Some(dci), _, _) = d {
                // Push DCI
                dci_buffer.push(dci);
            }
        }
    }
    frame.metrics.candidate_decode_ns = time.stop();

    // If we do not wish to expand candidates we are done
    if !EXPAND_CANDIDATES {
        // Sort by time, scs and lowest group, then forward dci
        dci_buffer.sort_by(|a, b| a.time_ns.cmp(&b.time_ns).then_with(|| a.scs.cmp(&b.scs)).then_with(|| a.lowest_group.cmp(&b.lowest_group)));
        for dci in dci_buffer.drain(..) {
            if tx.send(Ok(dci)).await.is_err() {
                return true;
            }
        }

        if metrics_tx.send(frame.metrics).await.is_err() {
            return true;
        }
        return false;
    }

    // Step 2: Iterate over all active groups remaining and try to find new candidates
    let time = TimeMeasurement::start();
    for bwp_id in 0..frame.bwps.len() {
        // We clone the most likely numerologies here, because we need to mutably borrow state below (performance is negligible here)
        let bwp = &frame.bwps[bwp_id];
        let scs = bwp.scs;
        let ext = bwp.extended;
        let offset_to_carrier = bwp.offset_to_carrier;
        let numerology_characteristics = Numerology::from_scs(scs as usize, ext).get_characteristics();
        let slots = numerology_characteristics.slots_per_subframe * SUBFRAMES_PER_FRAME;
        for slot in 0..slots {
            // First we try to find further solutions of the ones we found previously in the frame
            for (symbol, n_sym, groups, dci_len, shared) in found_candidates_buffer.iter() {
                let ind = slot * numerology_characteristics.symbols_per_slot + *symbol;
                let f = if (MAX_ERRORS_AND_FREE_AL_1 == 0 && n_sym * groups.len() == 6) || !cf_error_correction {
                    // Disable error correction
                    try_decode_candidate::<true, false>
                } else {
                    // Enable error correction
                    try_decode_candidate::<true, true>
                };
                if let Some(dci) = f(global_frame_counter, frame.sfn, scs, ext, ind, *n_sym, groups.as_slice(), &frame.bwps[bwp_id], &mut buffer[0], frame.cell_id, &[*dci_len], *shared, use_dmrs) {
                    // Remove candidate from active groups
                    remove_candidate_from_active(scs, ext, offset_to_carrier, ind, *n_sym, groups.as_slice(), &mut frame.bwps);
                    // Forward DCI
                    if tx.send(Ok(dci)).await.is_err() {
                        return true;
                    }
                }
            }

            // Explicit reborrow to make borrow checker happy
            let bwp = &mut frame.bwps[bwp_id];

            // Find new candidates
            for symbol in 0..numerology_characteristics.symbols_per_slot {
                for coreset_time_symbols in (1..min(4, numerology_characteristics.symbols_per_slot - symbol + 1)).rev() {
                    let ind = slot * numerology_characteristics.symbols_per_slot + symbol;
                    let mut search_vec = match coreset_time_symbols {
                        1 => bwp.symbols[ind].active_group_indexes.to_vec(),
                        2 => intersect_2(bwp.symbols[ind].active_group_indexes.as_slice(), bwp.symbols[ind + 1].active_group_indexes.as_slice()),
                        3 => intersect_3(bwp.symbols[ind].active_group_indexes.as_slice(), bwp.symbols[ind + 1].active_group_indexes.as_slice(), bwp.symbols[ind + 2].active_group_indexes.as_slice()),
                        _ => unreachable!("Coreset length is 1, 2 or 3")
                    };
                    let min_size = match coreset_time_symbols {
                        1 => 6,
                        2 => 3,
                        3 => 2,
                        _ => unreachable!("Coreset length is 1, 2 or 3")
                    };
                    if search_vec.len() < min_size {
                        continue;
                    }
                    #[cfg(feature = "pipeline_trace")]
                    let _ = log.send(PipelineLogEntry::of(crate::pipeline::pipeline::PipelineLogLevel::Trace, CF_MODULE, format!("Frame {} - Slot: {slot} Symbol: {symbol} N_sym: {coreset_time_symbols} ({ind}) - Searching at group indexes: {search_vec:?}", frame.sfn))).await;

                    let small_reg_bundle_freq_group_size = if coreset_time_symbols == 1 { 2 } else { 1 };
                    let big_reg_bundle_freq_group_size = 6 / coreset_time_symbols;

                    // Optimization: Try the aggregation layer that fits the input size first, if configured
                    let mut reorder = [0; 5];
                    let e = if cf_super_fast_mode {
                        &[]
                    } else {
                        search_al
                    };
                    let search_al = if TRY_FITTING_AL_FIRST {
                        let total_reg_groups = search_vec.len() * coreset_time_symbols;
                        let fitting_al = total_reg_groups / 6;
                        if fitting_al == 1 || fitting_al == 2 || fitting_al == 4 || fitting_al == 8 || fitting_al == 16 {
                            if fitting_al * 6 == total_reg_groups && search_al.contains(&fitting_al) {
                                // We have the perfect amount and the AL should be searched
                                let mut j = 1;
                                reorder[0] = fitting_al;
                                for i in 0..search_al.len() {
                                    if search_al[i] != fitting_al {
                                        reorder[j] = search_al[i];
                                        j += 1;
                                    }
                                }
                                if cf_super_fast_mode {
                                    // Only try the fitting AL
                                    &reorder[0..1]
                                } else {
                                    // Try all, but in better order
                                    &reorder[0..search_al.len()]
                                }
                            } else {
                                e
                            }
                        } else {
                            e
                        }
                    } else {
                        e
                    };

                    // Bruteforce through all possible combinations, but only if amount of
                    // combinations to check is doable. Rest will fall out over time (by
                    // different presence of PDCCH candidates).
                    for al in search_al {
                        // This is log2, but faster as a lookup table
                        let al_ind = match al {
                            1 => 0,
                            2 => 1,
                            4 => 2,
                            8 => 3,
                            16 => 4,
                            _ => panic!("Invalid aggregation layer! Must be 1, 2, 4, 8 or 16!")
                        };
                        let k = al * 6 / coreset_time_symbols;
                        const MAX_K: usize = 36 * 6;
                        while search_vec.len() >= k {
                            // All continuous elements
                            let sequences = Sequences::<MAX_K>::new(k, search_vec.len());
                            // Group-wise search (works best when search vec is multiple of group size, else last elements are ignored).
                            // Will be constructed below if required.

                            // Full brute-force of all combinations
                            let combinations = Combinations::<MAX_K>::new(k, search_vec.len());

                            // Call in order, tracking the maximum number of items to search before continuing to the next stage
                            let mut already_searched = sequences.len();
                            let mut m = perform_search::<MAX_K>(global_frame_counter, frame.sfn, scs, ext, bwp, sequences.into_par_iter(), k, *al, ind, coreset_time_symbols, &search_vec, frame.cell_id, cf_error_correction, use_dmrs);
                            if !cf_fast_mode {
                                if m.is_none() && (small_reg_bundle_freq_group_size == 6 || big_reg_bundle_freq_group_size == 6) {
                                    let g = grouped_combinations_6::<MAX_K>(k, search_vec.len());
                                    if let Some(len) = g.opt_len() && already_searched + len < MAX_COMBINATIONS_PER_SYMBOL_AND_AL[al_ind] {
                                        already_searched += len;
                                        m = perform_search::<MAX_K>(global_frame_counter, frame.sfn, scs, ext, bwp, g, k, *al, ind, coreset_time_symbols, &search_vec, frame.cell_id, cf_error_correction, use_dmrs);
                                    }
                                }
                                if m.is_none() && (small_reg_bundle_freq_group_size == 3 || big_reg_bundle_freq_group_size == 3) {
                                    let g = grouped_combinations_3::<MAX_K>(k, search_vec.len());
                                    if let Some(len) = g.opt_len() && already_searched + len < MAX_COMBINATIONS_PER_SYMBOL_AND_AL[al_ind] {
                                        already_searched += len;
                                        m = perform_search::<MAX_K>(global_frame_counter, frame.sfn, scs, ext, bwp, g, k, *al, ind, coreset_time_symbols, &search_vec, frame.cell_id, cf_error_correction, use_dmrs);
                                    }
                                }
                                if m.is_none() && (small_reg_bundle_freq_group_size == 2 || big_reg_bundle_freq_group_size == 2) {
                                    let g = grouped_combinations_2::<MAX_K>(k, search_vec.len());
                                    if let Some(len) = g.opt_len() && already_searched + len < MAX_COMBINATIONS_PER_SYMBOL_AND_AL[al_ind] {
                                        already_searched += len;
                                        m = perform_search::<MAX_K>(global_frame_counter, frame.sfn, scs, ext, bwp, g, k, *al, ind, coreset_time_symbols, &search_vec, frame.cell_id, cf_error_correction, use_dmrs);
                                    }
                                }
                                if m.is_none() && already_searched + combinations.len() < MAX_COMBINATIONS_PER_SYMBOL_AND_AL[al_ind] {
                                    m = perform_search::<MAX_K>(global_frame_counter, frame.sfn, scs, ext, bwp, combinations.into_par_iter(), k, *al, ind, coreset_time_symbols, &search_vec, frame.cell_id, cf_error_correction, use_dmrs);
                                }
                            }


                            // Check whether we have found a result
                            if let Some((dci, contained_groups)) = m {
                                // Exclude from search vec (and future search vecs)
                                subtract_vec_sorted(&mut search_vec, &contained_groups);
                                for sym in ind..ind + coreset_time_symbols {
                                    subtract_vec_sorted(&mut bwp.symbols[sym].active_group_indexes, &contained_groups);
                                }
                                let dci_len = dci.data.len();
                                let shared = dci.shared;
                                // Push dci to consumer and append to found candidates list
                                dci_buffer.push(dci);
                                found_candidates_buffer.push((symbol, coreset_time_symbols, contained_groups, dci_len, shared));
                            } else {
                                // No solution found (anymore)
                                break;
                            }
                        }
                    }
                }
            }
        }
        // Clear all found candidates from other scs and build candidate list
        let mut candidates = found_candidates_buffer.drain(..).map(|(symbol, n_sym, groups, dci_len, shared)| {
            remove_candidate_from_active(scs, ext, offset_to_carrier, symbol, n_sym, groups.as_slice(), &mut frame.bwps);
            Candidate {
                symbol,
                n_sym,
                known_dci_lens: vec![dci_len],
                shared,
                groups,
            }
        }).collect::<Vec<_>>();
        candidates.iter().for_each(|c| c.known_dci_lens.iter().for_each(|l| {
            if !dci_sizes_buffer.contains(l) {
                dci_sizes_buffer.push(*l);
            }
            dci_sizes_buffer.sort();
        }));
        // Push all found candidates to state for future reference
        state.push_extra_candidates(bwp_id, &mut candidates);
    }
    frame.metrics.candidate_find_ns = time.stop();

    // Sort by time, scs and lowest group, then forward dci (since we have DCI from two steps we need to order them!)
    dci_buffer.sort_by(|a, b| a.time_ns.cmp(&b.time_ns).then_with(|| a.scs.cmp(&b.scs)).then_with(|| a.lowest_group.cmp(&b.lowest_group)));
    for dci in dci_buffer.drain(..) {
        if tx.send(Ok(dci)).await.is_err() {
            return true;
        }
    }

    if metrics_tx.send(frame.metrics).await.is_err() {
        return true;
    }

    false
}

fn perform_search<const MAX_K: usize>(frame_counter: usize, sfn: usize, scs: u64, ext: bool, active_groups: &ActiveGroups, iter: impl ParallelIterator<Item=[usize; MAX_K]>, k: usize, al: usize, sym: usize, n_sym: usize, search_vec: &[usize], cell_id: u16, cf_error_correction: bool, use_dmrs: bool) -> Option<(DCI, Vec<usize>)> {
    let al_bits = al * BITS_AL_1;
    let f = if (MAX_ERRORS_AND_FREE_AL_1 == 0 && al == 1) || !cf_error_correction {
        // Disable error correction
        // We do not need to check group activity in this case, since we are only iterating
        // over active groups
        try_decode_candidate::<false, false>
    } else {
        // Enable error correction
        // We do not need to check group activity in this case, since we are only iterating
        // over active groups
        try_decode_candidate::<false, true>
    };
    iter.map(|x| {
        let mut dec = [0f32; BITS_AL_16];
        let mut indexes = [0; MAX_K];
        for i in 0..k {
            indexes[i] = search_vec[x[i]];
        }
        // Search without known DCI size
        f(frame_counter, sfn, scs, ext, sym, n_sym, &indexes[..k], active_groups, &mut dec[..al_bits], cell_id, &[], false, use_dmrs).map(|dci| (dci, indexes[..k].to_vec()))
    }).find_map_any(|x| x)
}

fn remove_candidate_from_active(scs: u64, ext: bool, offset_to_carrier: u16, sym: usize, n_sym: usize, groups: &[usize], active_groups_vec: &mut Vec<ActiveGroups>) {
    for active_groups in active_groups_vec.iter_mut() {
        let c_scs = active_groups.scs;
        let c_ext = active_groups.extended;
        let bandwidth_prbs = active_groups.bandwidth_re as usize / SUBCARRIERS_PER_PRB;
        // Point A is identical between all bwps, but offset_to_carrier may vary
        match scs.cmp(&c_scs) {
            Ordering::Less => {
                // More symbols in time, less frequency
                let freq_factor = c_scs / scs;
                // Filter all groups that lie outside of destination bwp
                let groups = groups.iter().map(|x| (*x as isize + offset_to_carrier as isize) / freq_factor as isize - active_groups.offset_to_carrier as isize)
                    .filter(|i| *i >= 0 && *i < bandwidth_prbs as isize)
                    .map(|i| i as usize)
                    .collect::<Vec<_>>();
                if groups.is_empty() {
                    continue;
                }
                // This is an approximation since it does not factor in varying cyclic prefix in normal cp contexts
                let time_start_ns = sym as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128;
                let time_end_ns = (sym + n_sym) as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128;
                let c_sym = (time_start_ns * (c_scs as usize / 15_000 * if c_ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128 / 10_000_000) as usize;
                let c_sym_end = (time_end_ns * (c_scs as usize / 15_000 * if c_ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128 / 10_000_000) as usize;
                // Delete
                for sym in c_sym..c_sym_end {
                    subtract_vec_sorted(&mut active_groups.symbols[sym].active_group_indexes, groups.as_slice());
                    if !active_groups.symbols[sym].llr_indexes.is_empty() {
                        groups.iter().for_each(|i| active_groups.symbols[sym].llr_indexes[*i] = usize::MAX);
                    }
                }
            }
            Ordering::Equal => {
                // Delete
                for sym in sym..sym + n_sym {
                    subtract_vec_sorted(&mut active_groups.symbols[sym].active_group_indexes, groups);
                    if !active_groups.symbols[sym].llr_indexes.is_empty() {
                        groups.iter().for_each(|i| active_groups.symbols[sym].llr_indexes[*i] = usize::MAX);
                    }
                }
            }
            Ordering::Greater => {
                // More symbols in frequency, less in time
                let freq_factor = scs / c_scs;
                // Filter all groups that lie outside of destination bwp
                let groups = groups.iter().map(|x| (*x as isize + offset_to_carrier as isize) * freq_factor as isize - active_groups.offset_to_carrier as isize)
                    .filter(|i| *i >= 0 && *i < bandwidth_prbs as isize)
                    .map(|i| i as usize)
                    .flat_map(|i| i..i + freq_factor as usize)
                    .collect::<Vec<_>>();
                if groups.is_empty() {
                    continue;
                }
                // This is an approximation since it does not factor in varying cyclic prefix in normal cp contexts
                let time_start_ns = sym as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128;
                let time_end_ns = (sym + n_sym) as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128;
                let c_sym = (time_start_ns * (c_scs as usize / 15_000 * if c_ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128 / 10_000_000) as usize;
                let c_sym_end = (time_end_ns * (c_scs as usize / 15_000 * if c_ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128 / 10_000_000) as usize;
                // Delete
                for sym in c_sym..c_sym_end {
                    subtract_vec_sorted(&mut active_groups.symbols[sym].active_group_indexes, groups.as_slice());
                    if !active_groups.symbols[sym].llr_indexes.is_empty() {
                        groups.iter().for_each(|i| active_groups.symbols[sym].llr_indexes[*i] = usize::MAX);
                    }
                }
            }
        }
    }
}

fn subtract_vec_sorted(a: &mut Vec<usize>, b: &[usize]) {
    let mut read = 0;
    let mut write = 0;
    let mut j = 0;

    while read < a.len() && j < b.len() {
        if a[read] < b[j] {
            a[write] = a[read];
            write += 1;
            read += 1;
        } else if a[read] > b[j] {
            j += 1;
        } else {
            read += 1;
            j += 1;
        }
    }

    // Remaining elements in a cannot appear in b
    let len = a.len();
    if read < len {
        let tail = len - read;
        a.copy_within(read..len, write);
        write += tail;
    }

    a.truncate(write);
}

fn try_decode_candidate<const CHECK_ACTIVE: bool, const ENABLE_ERROR_CORRECTION: bool>(frame_counter: usize, sfn: usize, scs: u64, ext: bool, symbol: usize, n_sym: usize, groups: &[usize], active_groups: &ActiveGroups, buffer: &mut [f32], cell_id: u16, known_dci_sizes: &[usize], known_shared: bool, use_dmrs: bool) -> Option<DCI> {
    if CHECK_ACTIVE {
        // Check if groups are active (llr indexes should all be initialized)
        for symbol in symbol..symbol + n_sym {
            if active_groups.symbols[symbol].llr_indexes.is_empty() {
                return None;
            }
        }
        for symbol in symbol..symbol + n_sym {
            if groups.iter().any(|i| active_groups.symbols[symbol].llr_indexes[*i] == usize::MAX) {
                // Not all groups of candidate are active
                return None;
            }
        }
    }
    let len = groups.len() * n_sym * PDCCH_SUBCARRIERS_PER_PRB * 2;
    let buffer = &mut buffer[..len];
    copy(active_groups, symbol, n_sym, groups, buffer);
    // Try to decode in every orientation
    let mut max_rnti = 0;
    let mut max_n_id = 0;
    let mut max_shared = false;
    let mut max_data = Vec::with_capacity(0);
    let mut max_errors = 0;
    let mut max_free = 0;
    let mut max = f32::MIN;
    for rot in 0..4 {
        let (mut rnti, n_id, shared, data, conf, errors, free) = try_decode::<ENABLE_ERROR_CORRECTION>(buffer, cell_id, known_dci_sizes, known_shared);
        if use_dmrs && shared {
            // Need to test DMRS for RNTI last bit flip
            const DMRS_PER_GROUP: usize = 6;
            let dmrs = (0..n_sym).map(|i| get_dmrs_ref_pdcch_var((symbol + i) as u64, n_id, (groups.last().expect("Expected at least one group")+1) * DMRS_PER_GROUP)).collect::<Vec<_>>();
            // Test all possible positions within CORESET (DMRS sequence starts at CORESET begin, not BWP begin)
            let mut corr = Complex32::zero();
            for test_offset in 0..=*groups.first().expect("Expected at least one group") {
                let mut corr_inner = Complex32::zero();
                for sym in 0..n_sym {
                    for index in groups {
                        let llrs = &active_groups.symbols[symbol+sym].dmrs_llrs[active_groups.symbols[symbol+sym].llr_indexes[*index]];
                        for i in 0..3 {
                            corr_inner += Complex32::new(llrs[i << 1], llrs[(i << 1) | 1]) * dmrs[sym][*index * 3 + i - test_offset * 3].conj();
                        }
                    }
                }
                if corr_inner.norm_sqr() > corr.norm_sqr() {
                    corr = corr_inner;
                }
            }
            // Rotate correlation the same as the original input buffer
            for _ in 0..rot {
                let re = corr.re;
                // re * 0 - im * 1
                corr.re = -corr.im;
                // re * 1 + im * 0
                corr.im = re;
            }
            if corr.re.is_negative() {
                // Invert last RNTI bit, since it will be flipped due to incorrect rotation
                rnti ^= 1;
            }
        }
        if buffer.len() > BITS_AL_1 && !data.is_empty() {
            // AL > 1 is likely to be correct and time intensive
            return Some(DCI {
                sfn,
                symbol,
                n_sym,
                al: buffer.len() / BITS_AL_1,
                time_ns: frame_counter as u128 * 10_000_000 + symbol as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128,
                lowest_group: groups[0],
                scs,
                errors,
                free,
                rnti,
                n_id,
                shared,
                classification: classify_dci(rnti, shared, data[0]),
                data,
            });
        }
        // On higher AL we only accept results with higher confidence and a maximum of errors and free bits
        if conf > max && errors + free < MAX_ERRORS_AND_FREE_AL_1 {
            max = conf;
            max_rnti = rnti;
            max_n_id = n_id;
            max_shared = shared;
            max_errors = errors;
            max_free = free;
            max_data = data;
        }
        if rot != 3 {
            // Rotate to next possible rotation
            rotate_90(buffer);
        }
    }
    if max_data.is_empty() {
        return None;
    }
    Some(DCI {
        sfn,
        symbol,
        n_sym,
        al: buffer.len() / BITS_AL_1,
        time_ns: frame_counter as u128 * 10_000_000 + symbol as u128 * 10_000_000 / (scs as usize / 15_000 * if ext { 12 } else { 14 } * SUBFRAMES_PER_FRAME) as u128,
        lowest_group: groups[0],
        scs,
        errors: max_errors,
        free: max_free,
        rnti: max_rnti,
        n_id: max_n_id,
        shared: max_shared,
        classification: classify_dci(max_rnti, max_shared, max_data[0]),
        data: max_data,
    })
}

pub fn copy(active_groups: &ActiveGroups, symbol: usize, n_sym: usize, indexes: &[usize], out: &mut [f32]) {
    let mut i = 0;
    const PER_GROUP: usize = 18;
    for offs in 0..n_sym {
        let symbol = symbol + offs;
        for index in indexes {
            out[i..i + PER_GROUP].copy_from_slice(&active_groups.symbols[symbol].llrs[active_groups.symbols[symbol].llr_indexes[*index]]);
            i += PER_GROUP;
        }
    }
}

pub fn try_decode<const ENABLE_ERROR_CORRECTION: bool>(vec: &[f32], cell_id: u16, known_dci_sizes: &[usize], known_shared: bool) -> (u16, u16, bool, Vec<u8>, f32, usize, usize) {
    let mat: &pdcch::Matrices = &pdcch::MATRICES;

    // Fast path: DCI size is known
    if !known_dci_sizes.is_empty() {
        for a in known_dci_sizes {
            if !known_shared && let Some((errors, free, scale, data, rnti, n_id)) = pdcch_decode_slice::<ENABLE_ERROR_CORRECTION>(mat, vec, *a + DCI_CRC_BITS) {
                if rnti == 0 || rnti > 0xFFF2 || n_id == cell_id {
                    // This hints at shared search space (non-UE specific), so likely a false positive
                    // More on this in TS 38.211 7.3.2.3
                    continue;
                }
                return (rnti, n_id, false, data, scale, errors, free);
            } else if known_shared && let Some((errors, free, scale, data, rnti, n_id)) = pdcch_decode_shared_slice::<ENABLE_ERROR_CORRECTION>(mat, vec, *a + DCI_CRC_BITS) {
                if rnti > 0 && rnti <= 0xFFF2 && n_id != cell_id {
                    // This hints at UE search space (non-shared), so likely a false positive
                    // More on this in TS 38.211 7.3.2.3
                    continue;
                }
                return (rnti, n_id, true, data, scale, errors, free);
            }
        }
        return (0, 0, false, Vec::with_capacity(0), f32::MIN, 0, 0);
    }

    // Normal path: Try all possible sizes
    let k_limit = match vec.len() {
        // 108 - 31 bits we wish to extract
        108 => 77,
        _ => 165
    };
    let mut max = f32::MIN;
    let mut max_rnti = 0;
    let mut max_n_id = 0;
    let mut max_data = Vec::with_capacity(0);
    let mut max_shared = false;
    let mut max_errors = 0;
    let mut max_free = 0;
    for k in 36..k_limit {
        if let Some((errors, free, scale, data, rnti, n_id)) = pdcch_decode_slice::<ENABLE_ERROR_CORRECTION>(mat, vec, k) {
            if rnti == 0 || rnti > 0xFFF2 || n_id == cell_id {
                // This hints at shared search space (non-UE specific), so likely a false positive
                // More on this in TS 38.211 7.3.2.3
                continue;
            }
            // Early abort on AL > 1
            if vec.len() > 108 {
                return (rnti, n_id, false, data, scale, errors, free);
            }
            let weight = scale / (errors + free) as f32;
            if weight > max {
                max = weight;
                max_rnti = rnti;
                max_n_id = n_id;
                max_data = data;
                max_shared = false;
                max_errors = errors;
                max_free = free;
            }
        } else if let Some((errors, free, scale, data, rnti, n_id)) = pdcch_decode_shared_slice::<ENABLE_ERROR_CORRECTION>(mat, vec, k) {
            if rnti > 0 && rnti <= 0xFFF2 && n_id != cell_id {
                // This hints at UE search space (non-shared), so likely a false positive
                // More on this in TS 38.211 7.3.2.3
                continue;
            }
            // Early abort on AL > 1
            if vec.len() > 108 {
                return (rnti, n_id, true, data, scale, errors, free);
            }
            let weight = scale / (errors + free) as f32;
            if weight > max {
                max = weight;
                max_rnti = rnti;
                max_n_id = n_id;
                max_data = data;
                max_shared = true;
                max_errors = errors;
                max_free = free;
            }
        }
    }
    (max_rnti, max_n_id, max_shared, max_data, max, max_errors, max_free)
}

#[inline]
fn rotate_90(data: &mut [f32]) {
    match data.len() {
        decoder::BITS_AL_1 => rotate_90_simd::<64>(data),
        decoder::BITS_AL_2 => rotate_90_simd::<128>(data),
        decoder::BITS_AL_4 => rotate_90_simd::<256>(data),
        decoder::BITS_AL_8 => rotate_90_simd::<256>(data),
        decoder::BITS_AL_16 => rotate_90_simd::<512>(data),
        _ => unreachable!()
    }
}

#[inline(always)]
fn rotate_90_simd<const SIMD_SIZE: usize>(data: &mut [f32]) {
    let (c, r) = data.as_chunks_mut::<SIMD_SIZE>();
    for c in c {
        for x in c.chunks_exact_mut(2) {
            let re = x[0];
            // re * 0 - im * 1
            x[0] = -x[1];
            // re * 1 + im * 0
            x[1] = re;
        }
    }
    for x in r.chunks_exact_mut(2) {
        let re = x[0];
        // re * 0 - im * 1
        x[0] = -x[1];
        // re * 1 + im * 0
        x[1] = re;
    }
}

pub fn intersect_2(first: &[usize], second: &[usize]) -> Vec<usize> {
    let mut i = 0;
    let mut j = 0;

    let mut ret = Vec::with_capacity(first.len().min(second.len()));

    while i < first.len() && j < second.len() {
        let a = first[i];
        let b = second[j];

        if a < b {
            i += 1;
        } else if a > b {
            j += 1;
        } else {
            ret.push(a);
            i += 1;
            j += 1;
        }
    }

    ret
}

pub fn intersect_3(first: &[usize], second: &[usize], third: &[usize]) -> Vec<usize> {
    let mut i = 0;
    let mut j = 0;
    let mut k = 0;

    let mut ret = Vec::with_capacity(first.len().min(second.len()).min(third.len()));

    while i < first.len() && j < second.len() && k < third.len() {
        let a = first[i];
        let b = second[j];
        let c = third[k];

        if a == b && b == c {
            ret.push(a);
            i += 1;
            j += 1;
            k += 1;
        } else if a <= b && a <= c {
            i += 1;
        } else if b <= a && b <= c {
            j += 1;
        } else {
            k += 1;
        }
    }

    ret
}