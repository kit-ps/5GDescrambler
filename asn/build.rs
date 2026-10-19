use rasn_compiler::OutputMode;
use rasn_compiler::prelude::*;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub fn main() {
    println!("cargo:rerun-if-changed=definitions/");
    fs::create_dir_all(PathBuf::from("./gen")).unwrap();
    let mut paths = Vec::new();
    for file in fs::read_dir("./definitions/").unwrap() {
        let file = file.unwrap();
        if file.file_name().to_str().unwrap().ends_with(".asn1") {
            paths.push(file.path());
        }
    }
    let config: <RasnBackend as Backend>::Config = Default::default();
    //config.type_annotations.push(String::from("#[derive(Default, Clone)]"));
    let path = PathBuf::from("./gen/generated_asn.rs");
    let compiler = Compiler::<RasnBackend, _>::new_with_config(config)
        // add several ASN1 source files
        .add_asn_sources_by_path(paths.iter())
        // set an output path for the generated rust code
        .set_output_mode(OutputMode::SingleFile(path.clone()));
    match compiler.compile()
    {
        Ok(warnings) => {
            if !warnings.is_empty() {
                println!(
                    "cargo::warning=Warnings while compiling ASN definitions: {:?}",
                    warnings
                );
            }
        }
        Err(error) => {
            panic!("Error while compiling ASN definitions: {:?}", error)
        }
    }


    // TODO Submit both issues worked-around here to the ASN1 crate we use and fix the issue centrally
    // Perform some post edits to work around compiler bugs
    let d = fs::read_to_string(path.clone()).expect("Expected to be able to read ASN1 compiler output");

    // Fix SetupRelease bug
    let d = d.replace("SetupRelease, ", "");

    // Fix default function naming
    let d = d
        .replace("filter_config_cli_r16_filter_coefficient_srs_rsrp_r16_default()", "filter_config_clir16_filter_coefficient_srs_rsrp_r16_default()")
        .replace("filter_config_cli_r16_filter_coefficient_cli_rssi_r16_default()", "filter_config_clir16_filter_coefficient_cli_rssi_r16_default()")

        .replace("q_offset_range_list_rsrp_offset_ssb_default()", "qoffset_range_list_rsrp_offset_ssb_default()")
        .replace("q_offset_range_list_rsrq_offset_ssb_default()", "qoffset_range_list_rsrq_offset_ssb_default()")
        .replace("q_offset_range_list_sinr_offset_ssb_default()", "qoffset_range_list_sinr_offset_ssb_default()")
        .replace("q_offset_range_list_rsrp_offset_csi_rs_default()", "qoffset_range_list_rsrp_offset_csi_rs_default()")
        .replace("q_offset_range_list_rsrq_offset_csi_rs_default()", "qoffset_range_list_rsrq_offset_csi_rs_default()")
        .replace("q_offset_range_list_sinr_offset_csi_rs_default()", "qoffset_range_list_sinr_offset_csi_rs_default()")

        .replace("quantity_config_utra_fdd_r16_filter_coefficient_rscp_r16_default()", "quantity_config_utrafddr16_filter_coefficient_rscp_r16_default()")
        .replace("quantity_config_utra_fdd_r16_filter_coefficient_ec_no_r16_default()", "quantity_config_utrafddr16_filter_coefficient_ec_no_r16_default()")

        .replace("sl_quantity_config_r16_sl_filter_coefficient_dmrs_r16_default()", "slquantity_config_r16_sl_filter_coefficient_dmrs_r16_default()");

    let mut f = File::create(path).expect("Expected to be able to open ASN1 compiler output for writing");
    f.write_all(d.as_bytes()).expect("Expected to be able to write ASN1 compiler output")
}
