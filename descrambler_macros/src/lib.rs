extern crate proc_macro;

use proc_macro::TokenStream;
use serde::Deserialize;
use serde_with::serde_as;
use serde_with::DisplayFromStr;
use std::fs;

#[derive(Deserialize)]
struct PLMNData {
    pub data: Vec<PLMN>,
}

#[serde_as]
#[derive(Deserialize)]
struct PLMN {
    #[serde_as(as = "DisplayFromStr")]
    mcc: u16,
    #[serde_as(as = "DisplayFromStr")]
    mnc: u16,
    iso: String,
    country: String,
    #[serde_as(as = "DisplayFromStr")]
    #[serde(rename = "countryCode")]
    country_code: u16,
    network: String,
}


#[proc_macro]
pub fn plmn_list(_item: TokenStream) -> TokenStream {
    let s = match fs::read_to_string("descrambler_macros/cache/plmn_list.json") {
        Ok(content) => content,
        Err(_) => return "const PLMN_LIST: [PLMN; 0] = [];".parse().unwrap(),
    };

    let s: PLMNData = serde_json::from_str(&s).expect("Invalid data in descrambler_macros/cache/plmn_list.json!");

    let networks = s.data.iter().map(|s| {
        format!("PLMN {{ mcc: {}, mnc: {}, iso: \"{}\", country: \"{}\", country_code: {}, name: \"{}\" }}", s.mcc, s.mnc, s.iso, s.country, s.country_code, s.network)
    }).collect::<Vec<_>>();

    format!("const PLMN_LIST: [PLMN; {}] = [{}];", networks.len(), networks.join(", ")).parse().unwrap()
}