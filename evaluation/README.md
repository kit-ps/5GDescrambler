# Evaluation

## Requirements
### Hardware Requirements
There are no specific hardware requirements, e.g. a laptop is sufficient.
Due to some experiments being compute-heavy, we recommend using a recent CPU model.
We recommend using at least 32 GiB RAM when building 5GDescrambler, as included in some experiments.

We performed evaluation for the estimated times below on a PC with:
 - AMD Ryzen 7 7700 processor
 - 32 GiB RAM + 8 GiB swap

### Software Requirements
We performed evaluation on Manjaro Linux and Ubuntu 26, but any docker-compatible environment should suffice.
SDR functionality might be impaired when running on WSL or other environments with special USB device or host network handling,
but is not required for evaluation.

When using docker, only docker and docker buildx are required to be installed.

When **not** using a docker environment, the following packages are required:
```text
# Basic packages
rust (v1.98+)
rust-script
libsoapysdr-dev
libsoapysdr0.8
soapysdr-tools

# Optional soapysdr modules (depends on SDR in use)
soapysdr0.8-module-*
# For UHD support
soapysdr0.8-module-uhd
# Extra required packages for uhd support
libuhd-dev
uhd-host

# Required for discovery of SDRs (most likely preinstalled already)
dbus
avahi-daemon

# Required for building of some rust-scripts
pkg-config
```
When **not** using docker, to install packages required for building on Debian based distros, run:
```bash
# Install rust and rust-script
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add rustfmt
cargo install rust-script
# Install soapysdr, pkg-config and libfontconfig
apt-get update && apt-get install -y --no-install-recommends \
    libsoapysdr-dev \
    libsoapysdr0.8 \
    soapysdr-tools \
    pkg-config \
    libfontconfig1-dev
```

## Setup
### Docker
Please use our `start_docker.sh` script as explained in [README.md](../README.md) to obtain an environment pre-setup for evaluation.
This requires a docker-compatible environment to be installed on the system.
### Local
Please follow the installation instructions and requirements in [README.md](../README.md) and make sure to add the binary to your PATH.

## Sample acquisition
Please obtain our samples as explained in [README.md](../README.md), and extract them to `descrambler` so the file structure looks like this:
```text
5GDescrambler
├── descrambler
│   ├── samples
│   │   ├── compare_output.rs
│   │   ├── 5GSniffer_srsRAN
│   │   │   ├── 5g_sn_1.sigmf-collection
│   │   │   ├── 5g_sn_2.sigmf-collection
│   │   │   └── ...
│   │   ├── vendor_1_redacted
│   │   │   ├── vendor_1_redacted_flood.sigmf-collection
│   │   │   └── ...
│   │   ├── vendor_2_redacted
│   │   │   ├── flood_1
│   │   │   │   ├── vendor_2_redacted_flood_1.sigmf-collection
│   │   │   │   └── ...
│   │   │   ├── flood_2
│   │   │   │   ├── vendor_2_redacted_flood_2.sigmf-collection
│   │   │   │   └── ...
│   │   │   └── flood_3
│   │   │       ├── vendor_2_redacted_flood_3.sigmf-collection
│   │   │       └── ...
│   │   └── OAI
│   │       ├── oai_flood.sigmf-collection
│   │       ├── oai_ping.sigmf-collection
│   │       └── ...
│   └── ...
└── ...
```
## Performing evaluation
If using docker, we assume you execute all following commands inside a freshly started 5GDescrambler docker container. For a local setup, we assume to start in the repository root.

First, change to the evaluation directory:
```bash
cd evaluation
```
You can then run the individual scripts, including plots and tables, to reproduce the results.

### Claims
The main claims from our publication are 1) robustness of active group detection, 2) robustness of DCI decoding and 3) claims on robustness and performance over the individual steps combined in a pipeline.
These are evaluated by Experiments 1-3 below.

Smaller claims are robustness and performance compared to conventional CA-SCL polar decoding (experiment 2b & 4), performance of individual DCI encoding and decoding using our technique (experiment 4) and the characteristics of encoding and decoding matrices (experiment 5).
We thus marked these experiments optional.

### Automatic evaluation
- Time (with experiment 2 and 2a with 10,000 runs): Up to 3 days compute time, 7 minutes manual work
- Time (with experiment 2 and 2a with 1,000 runs): ~7 hours compute time, 7 minutes manual work

In order to run all experiments mentioned below automatically, you can run the full evaluation script. 
Results are returned in the format described below, or if they are originally returned to std, then they are written to file instead.
```bash
./perform_evaluation.sh
```

### Experiment 1: Active group detection
- Time: 28 minutes compute

This experiment reproduces the characteristics claimed on active group detection.
```shell
./active_group/test_active_groups.rs
./active_group/plot_active_groups.rs
```

Results are placed in `active_group/results` (results are prepopulated). The paper showcases `f1_macro_active_groups_test` and `std_angle_active_groups_test`.

### Experiment 2: DCI decoding
- Time: 34h for 10,000 runs per configuration (as utilized in publication), 3h20 for 1,000 runs per configuration

For DCI decoding (also experiment 2a) we strongly recommend reducing the amount of runs per configuration to e.g. 1,000, since running evaluation on 10,000 runs per configuration is time expensive.
Please note however that when reducing runs to e.g. 1,000, changes in BLER plots such as increased jitter will be visible due to the reduced amount of data points, especially below $10^{-2}$. 

```shell
./dci/set_runs.sh 10000
./dci/test_dci.rs --both all
./dci/plot_dci.rs --both all
```

Results are placed in `dci/results` (results are prepopulated for 10,000 runs). The paper showcases `bler_non_shared_al_comparison`, `bler_shared_al_comparison`, `bler_non_shared_al_one` and `bler_non_shared_al_one_known_params` in their polar (L=8) comparison variants (see experiment 2b).

### Optional Experiment 2a: Polar DCI decoding
- Time: 25h for 10,000 runs per configuration (as utilized in publication), 2h30 for 1,000 runs per configuration

To test robustness of CA-SCL polar decoding with list size 8 for comparison, run the following:
```shell
./dci/set_runs.sh 10000
./dci/test_dci.rs --both all polar 8
./dci/plot_dci.rs --both all polar 8
```
Results are placed in `dci/results` (results are prepopulated for 10,000 runs). The paper showcases the figures mentioned above in the polar (L=8) variant produced by this experiment, where robustness of the polar decoder is plotted additionally as a dotted line.
The dotted line and thus this experiment is optional however.

### Experiment 3: Sniffing
- Time: 1 minute compute, 5 minutes manual work

To evaluate sniffing, use the `sniffing/test_sniffing.rs` script.

```shell
# Usage
./sniffing/test_sniffing.rs <awgn> <disable_dmrs> <path/to/x> [args]
# awgn: true or false. If enabled, introduces varying AWGN to the sample, summarizing behaviour over decreasing signal quality.
# disable_dmrs: true or false. If disabled, least significant RNTI bit will become ambiguous in shared search spaces.
# x: Expects to be pointed to a <x>.sigmf-collection file with <x>.sigmf-meta, <x>.sigmf-data and <x>.expected-output present next to it.
# args: Additional arguments are passed to the descrambler binary.

./sniffing/test_sniffing.rs false true ../descrambler/samples/5GSniffer_srsRAN/5g_sn_1 --band 71
./sniffing/test_sniffing.rs false true ../descrambler/samples/5GSniffer_srsRAN/5g_sn_2 --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/OAI/oai_flood --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/OAI/oai_ping --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_1_redacted/vendor_1_redacted_flood --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_1/vendor_2_redacted_flood_1 --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_2/vendor_2_redacted_flood_2 --band 78
./sniffing/test_sniffing.rs false true ../descrambler/samples/vendor_2_redacted/flood_3/vendor_2_redacted_flood_3 --band 78

./sniffing/table_sniffing.rs
```

The second boolean value passed to the script indicates whether DMRS correlation to disambiguate the least significant RNTI bit
in shared search spaces should be disabled. Therefore, when setting this value to `false` as suggested above, no RNTI
LSB mismatches should occur anymore. Usage of this feature is explicitly optional however, and has not been used in the paper.

Results are placed in `sniffing/results` (results are prepopulated). Please note that the resulting table entries for the commercial vendors (vendor 1 redacted and vendor 2 redacted) 
need to be adapted to reflect the difference from their `log_comparison.txt` files as well:
- The amount of missed DCI that are not in expected output need to be added to the false negatives column and the reference count column (0 for vendor 1; 2, 17 and 28 for vendor 2 samples).
- The amount of additional DCI that do not match RNTIs present in UE log and are not special RNTIs (>65520) need to be added to the false positives. 
This only appears in vendor_2_redacted samples however, where it was manually verified that the other RNTIs not in UE log were indeed registered on the cell and do thus not constitute false positives, but either false negatives by the Amarisoft UEs or RNTI not allocated to the Amarisoft UEs. 
Therefore, false positives were verified to always be 0.

### Optional Experiment 4: Benchmarks
- Time: 10 minutes build, 25 minutes compute

In order to benchmark the individual components of DCI encoding, decoding using our technique and CA-SCL polar decoding, run the following:
```bash
cd .. # Run from main directory of repository
cargo bench
```
It can happen that the build fails due to running out of memory while computing the matrices or ASN structures (SIGKILL). In that case, please check the available memory and restart the experiment.

When utilizing the automatic evaluation script, results are placed in `auto/bench.txt`. Benches are prepopulated across different sources:
- [Matrix and vector operations](../bit_types/benches/results.md)
- [DCI encoding and decoding using our technique](../pdcch/benches/results.md)
- [DCI decoding using CA-SCL polar decoding](../phys/benches/results.md)

### Optional Experiment 5: Matrix characteristics
- Time: 3 minutes build, 4 seconds compute

In order to reproduce matrix characteristics as stated by our publication, you can execute the following test script:
```bash
cd .. # Run from main directory of repository
cargo test --package pdcch --lib -- rank_nullity::compute_rank_nullity --exact --no-capture
```
When utilizing the automatic evaluation script, results are placed in `auto/matrix_characteristics.md`. A summarized version of the output can be found [here](../pdcch/rank_and_nullity_summarized.md) and the full output [here](../pdcch/rank_and_nullity.md).
