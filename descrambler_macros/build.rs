use std::fs;
use std::process::Command;

pub fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=cache");

    if !fs::exists("cache/plmn_list.json").expect("Could not check for existence of cache/plmn_list.json") {
        // Update plmn list
        if !Command::new("sh").arg("update_plmn_list.sh").status().expect("Failed to run plmn list fetch command!").success() {
            println!("cargo:warning=Failed to update plmn_list from mcc-mnc.com! Make sure you have a working connection to the internet to fetch the list!")
        }
    }
}