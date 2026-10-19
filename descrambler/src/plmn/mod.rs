use descrambler_macros::plmn_list;

plmn_list!();
pub const DEFAULT_PLMN: PLMN = PLMN {
    mcc: 0,
    mnc: 0,
    iso: "?",
    country: "?",
    country_code: 0,
    name: "?",
};

pub struct PLMN {
    pub mcc: u16,
    pub mnc: u16,
    pub iso: &'static str,
    pub country: &'static str,
    pub country_code: u16,
    pub name: &'static str,
}

pub fn get_plmn_info(mcc: u16, mnc: u16) -> Option<&'static PLMN> {
    PLMN_LIST.iter().find(|&plmn| plmn.mcc == mcc && plmn.mnc == mnc).map(|v| v as _)
}