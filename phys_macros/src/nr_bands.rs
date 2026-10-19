use proc_macro::TokenStream;
use std::fs;

const MHZ_F64: f64 = 1_000_000f64;
const MHZ_U64: u64 = 1_000_000u64;
/// Below this threshold lies FR2-1, above FR2-2
const FR2_FREQ_DIV: u64 = 52_600u64;

#[repr(u8)]
#[derive(Debug)]
enum FrequencyRange {
    FR1,
    FR2_1,
    FR2_2,
}

struct Band {
    name: String,
    mode: String,
    frequency_range: FrequencyRange,
    lower_dl: u64,
    higher_dl: u64,
    lower_ul: u64,
    higher_ul: u64,
    duplex_spacing: Option<i64>,
    supported_raster_deltas: Vec<u64>,
    supported_bandwidths: Vec<Vec<u64>>,
}

pub fn nr_bands() -> TokenStream {
    let r1 = fs::read_to_string("phys_macros/resources/bands/bands_fr1.csv")
        .expect("Could not read FR1 bands!");
    let r2 = fs::read_to_string("phys_macros/resources/bands/bands_fr2.csv")
        .expect("Could not read FR2 bands!");
    let fr1 = r1.trim().lines().collect::<Vec<_>>();
    let fr2 = r2.trim().lines().collect::<Vec<_>>();

    let mut bands = Vec::<Band>::with_capacity(fr1.len() + fr2.len());

    for f in fr1 {
        let columns = f.split(";").collect::<Vec<_>>();
        let name = columns[0].trim().to_string();
        let mode = columns[2].trim().to_string();
        let raster_frequency_delta = columns[3].trim().parse::<u64>().unwrap() * MHZ_U64;

        if let Some(b) = bands.last_mut()
            && b.name == name {
                b.supported_raster_deltas.push(raster_frequency_delta);
                continue;
            }

        let lower_dl = (columns[5].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let higher_dl = (columns[6].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let lower_ul = (columns[9].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let higher_ul = (columns[10].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let duplex_spacing = columns[15].trim().parse::<f64>().map(|x| (x * MHZ_F64) as i64).ok();
        let mut supported_bandwidths_scs_15 = Vec::with_capacity(16);
        for i in 18..34 {
            if let Ok(bw) = columns[i].trim().parse::<u64>() {
                supported_bandwidths_scs_15.push(bw * MHZ_U64);
            }
        }
        let mut supported_bandwidths_scs_30 = Vec::with_capacity(16);
        for i in 35..51 {
            if let Ok(bw) = columns[i].trim().parse::<u64>() {
                supported_bandwidths_scs_30.push(bw * MHZ_U64);
            }
        }
        let mut supported_bandwidths_scs_60 = Vec::with_capacity(16);
        for i in 52..68 {
            if let Ok(bw) = columns[i].trim().parse::<u64>() {
                supported_bandwidths_scs_60.push(bw * MHZ_U64);
            }
        }

        bands.push(Band {
            name,
            mode,
            frequency_range: FrequencyRange::FR1,
            lower_dl,
            higher_dl,
            lower_ul,
            higher_ul,
            duplex_spacing,
            supported_raster_deltas: vec![raster_frequency_delta],
            supported_bandwidths: vec![supported_bandwidths_scs_15, supported_bandwidths_scs_30, supported_bandwidths_scs_60],
        })
    }

    for f in fr2 {
        let columns = f.split(";").collect::<Vec<_>>();
        let name = columns[0].trim().to_string();
        let mode = columns[2].trim().to_string();
        let raster_frequency_delta = columns[3].trim().parse::<u64>().unwrap() * MHZ_U64;

        if let Some(b) = bands.last_mut()
            && b.name == name {
                b.supported_raster_deltas.push(raster_frequency_delta);
                continue;
            }
        let lower_dl = (columns[5].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let higher_dl = (columns[6].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let lower_ul = (columns[9].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let higher_ul = (columns[10].trim().parse::<f64>().unwrap_or(0f64) * MHZ_F64) as u64;
        let duplex_spacing = columns[15].trim().parse::<f64>().map(|x| (x * MHZ_F64) as i64).ok();
        let mut supported_bandwidths_scs_60 = Vec::with_capacity(4);
        for i in 18..22 {
            if let Ok(bw) = columns[i].trim().parse::<u64>() {
                supported_bandwidths_scs_60.push(bw * MHZ_U64);
            }
        }
        let mut supported_bandwidths_scs_120 = Vec::with_capacity(4);
        for i in 23..27 {
            if let Ok(bw) = columns[i].trim().parse::<u64>() {
                supported_bandwidths_scs_120.push(bw * MHZ_U64);
            }
        }

        bands.push(Band {
            name,
            mode,
            frequency_range: if lower_dl < FR2_FREQ_DIV * MHZ_U64 { FrequencyRange::FR2_1 } else { FrequencyRange::FR2_2 },
            lower_dl,
            higher_dl,
            lower_ul,
            higher_ul,
            duplex_spacing,
            supported_raster_deltas: vec![raster_frequency_delta],
            supported_bandwidths: vec![supported_bandwidths_scs_60, supported_bandwidths_scs_120],
        })
    }

    let mut s = String::with_capacity(bands.len() * 1024);

    for band in &bands {
        s.push_str(format!("pub const BAND_{}: Band = Band {{ name: \"{}\", number: {}, mode: DuplexMode::{}{}, frequency_range: FrequencyRange::{:?}, lower_dl: {}, higher_dl: {}, lower_ul: {}, higher_ul: {}, supported_raster_deltas: &{:?}, supported_bandwidths: {} }};\n",
                           band.name.to_uppercase(),
                           band.name,
                           &band.name[1..],
                           band.mode,
                           if let Some(x) = band.duplex_spacing {
                               format!("({})", x)
                           } else {
                               String::new()
                           },
                           band.frequency_range,
                           band.lower_dl,
                           band.higher_dl,
                           band.lower_ul,
                           band.higher_ul,
                           band.supported_raster_deltas,
                           format!("{:?}", band.supported_bandwidths).replace("[", "&["),
        ).as_str());
    }

    s.push_str(format!("pub const BANDS: [&Band; {}] = [", bands.len()).as_str());

    for band in bands {
        s.push_str(format!("&BAND_{}, ", band.name.to_uppercase()).as_str());
    }

    s.push_str("];");

    s.parse().unwrap()
}
