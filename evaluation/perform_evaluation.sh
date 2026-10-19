#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname $0)"

RUN_EXP1=true
RUN_EXP2=true
RUN_EXP2A=false
RUN_EXP3=true
RUN_EXP4=false
RUN_EXP5=false

RUNS_EXP2=10000
RUNS_EXP2A=10000

ask_yes_no() {
    local prompt="$1"
    local default="$2"
    local suffix="[y/N]"
    [[ "$default" == "true" ]] && suffix="[Y/n]"
    local reply
    read -r -p "$prompt $suffix " reply || true
    reply="${reply,,}" # lowercase
    if [[ -z "$reply" ]]; then
        echo "$default"
    elif [[ "$reply" == "y" || "$reply" == "yes" ]]; then
        echo "true"
    elif [[ "$reply" == "n" || "$reply" == "no" ]]; then
        echo "false"
    else
        echo "$default"
    fi
}

ask_number() {
    local prompt="$1"
    local default="$2"
    local reply
    read -r -p "$prompt [$default] " reply || true
    if [[ -z "$reply" ]]; then
        echo "$default"
    elif [[ "$reply" =~ ^[0-9]+$ ]]; then
        echo "$reply"
    else
        echo "Invalid number, using default ($default)" >&2
        echo "$default"
    fi
}

# Experiment selection
echo "Select experiments to run. Non-optional experiments are pre-selected (press Enter to accept defaults)."
echo

RUN_EXP1=$(ask_yes_no "Run Experiment 1 (Active group detection, ~28 min)?" "$RUN_EXP1")
RUN_EXP2=$(ask_yes_no "Run Experiment 2 (DCI decoding)?" "$RUN_EXP2")
if [[ "$RUN_EXP2" == "true" ]]; then
    RUNS_EXP2=$(ask_number "  Number of runs per configuration for Experiment 2 (10000 [as in paper] ~34h; 1000 ~3h20)" "$RUNS_EXP2")
    RUNS_EXP2A=$RUNS_EXP2
fi
RUN_EXP2A=$(ask_yes_no "Run Optional Experiment 2a (Polar DCI decoding comparison)?" "$RUN_EXP2A")
if [[ "$RUN_EXP2A" == "true" ]]; then
    RUNS_EXP2A=$(ask_number "  Number of runs per configuration for Experiment 2a (10000 [as in paper] ~25h; 1000 ~2h30)" "$RUNS_EXP2A")
fi
RUN_EXP3=$(ask_yes_no "Run Experiment 3 (Sniffing, ~1 min)?" "$RUN_EXP3")
RUN_EXP4=$(ask_yes_no "Run Optional Experiment 4 (Benchmarks, ~25 min build, ~10 min compute)?" "$RUN_EXP4")
RUN_EXP5=$(ask_yes_no "Run Optional Experiment 5 (Matrix characteristics, ~3 min build, ~4 sec compute)?" "$RUN_EXP5")

echo
echo "Selected experiments"
echo "Experiment 1  (active group detection): $RUN_EXP1"
echo "Experiment 2  (DCI decoding, runs=$RUNS_EXP2): $RUN_EXP2"
echo "Experiment 2a (polar DCI decoding, runs=$RUNS_EXP2A): $RUN_EXP2A"
echo "Experiment 3  (sniffing): $RUN_EXP3"
echo "Experiment 4  (benchmarks): $RUN_EXP4"
echo "Experiment 5  (matrix characteristics): $RUN_EXP5"
CONFIRM=$(ask_yes_no "Proceed with this selection?" "true")
if [[ "$CONFIRM" != "true" ]]; then
    echo "Aborted."
    exit 0
fi

START_TIME=$(date +%s)

# Experiment 1: Active group detection
if [[ "$RUN_EXP1" == "true" ]]; then
    echo "> Experiment 1: Active group detection"
    chmod +x ./active_group/*.rs
    ./active_group/test_active_groups.rs
    ./active_group/plot_active_groups.rs
    echo "Results written to active_group/results"
fi

# Experiment 2: DCI decoding
if [[ "$RUN_EXP2" == "true" ]]; then
    echo "> Experiment 2: DCI decoding (runs=$RUNS_EXP2)"
    chmod +x ./dci/*.sh
    chmod +x ./dci/*.rs
    ./dci/set_runs.sh "$RUNS_EXP2"
    ./dci/test_dci.rs --both all
    ./dci/plot_dci.rs --both all
    echo "Results written to dci/results"
fi

# Optional Experiment 2a: Polar DCI decoding
if [[ "$RUN_EXP2A" == "true" ]]; then
    echo "> Optional Experiment 2a: Polar DCI decoding (runs=$RUNS_EXP2A)"
    chmod +x ./dci/*.sh
    chmod +x ./dci/*.rs
    ./dci/set_runs.sh "$RUNS_EXP2A"
    ./dci/test_dci.rs --both all polar 8
    ./dci/plot_dci.rs --both all polar 8
    echo "Results written to dci/results"
fi

# Experiment 3: Sniffing
if [[ "$RUN_EXP3" == "true" ]]; then
    echo "> Experiment 3: Sniffing"
    chmod +x ./dci/*.rs
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/5GSniffer_srsRAN/5g_sn_1 --band 71
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/5GSniffer_srsRAN/5g_sn_2 --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/OAI/oai_flood --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/OAI/oai_ping --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_1_redacted/vendor_1_redacted_flood --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_1/vendor_2_redacted_flood_1 --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_2/vendor_2_redacted_flood_2 --band 78
    ./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_3/vendor_2_redacted_flood_3 --band 78
    ./sniffing/table_sniffing.rs
    echo "Results placed in sniffing/results"
    echo "Note: Commercial vendor (redacted vendor 1 & 2) result tables require manual"
    echo "      adaptation as described in the evaluation README."
fi

# Optional Experiment 4: Benchmarks
if [[ "$RUN_EXP4" == "true" ]]; then
    echo "> Optional Experiment 4: Benchmarks"
    (cd "$(test -f /.dockerenv && echo ../src || echo ..)" && cargo clean && cargo bench --no-run)
    (cd "$(test -f /.dockerenv && echo ../src || echo ..)" && cargo bench) | tee "auto/bench.txt"
    echo "Benchmark results written to auto/bench.txt"
fi

# Optional Experiment 5: Matrix characteristics
if [[ "$RUN_EXP5" == "true" ]]; then
    echo "> Optional Experiment 5: Matrix characteristics"
    (cd "$(test -f /.dockerenv && echo ../src || echo ..)" && cargo clean && cargo test --no-run --package pdcch --lib -- rank_nullity::compute_rank_nullity --exact)
    (cd "$(test -f /.dockerenv && echo ../src || echo ..)" && cargo test --package pdcch --lib -- rank_nullity::compute_rank_nullity --exact --no-capture) \
    > "auto/matrix_characteristics.md"
    echo "Matrix characteristics written to auto/matrix_characteristics.md"
fi

END_TIME=$(date +%s)
ELAPSED=$(( END_TIME - START_TIME ))
echo "Evaluation complete in $(( ELAPSED / 3600 ))h $(( (ELAPSED % 3600) / 60 ))m $(( ELAPSED % 60 ))s"
