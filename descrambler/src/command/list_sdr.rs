#![cfg(feature = "sdr")]

use crate::command::OutputFormat;

pub fn list_sdrs(export_format: OutputFormat) -> Result<(), String> {
    if export_format == OutputFormat::Text {
        println!("Listing SDRs...");
    }
    match soapysdr::enumerate("") {
        Ok(sdrs) => {
            if sdrs.is_empty() {
                match export_format {
                    OutputFormat::Text => println!("No SDRs have been found"),
                    OutputFormat::JSON => println!("{{}}")
                }
                Ok(())
            } else {
                match export_format {
                    OutputFormat::Text => {
                        println!("The following SDRs have been found:");
                        for sdr in sdrs {
                            println!("SN: \"{}\" - Label: \"{}\"", sdr.get("serial").unwrap_or("<NO SERIAL>"), sdr.get("label").unwrap_or("<NO LABEL>"));
                        }
                    }
                    OutputFormat::JSON => {
                        println!("{{");
                        println!("{}", sdrs.iter().map(|sdr| {
                            let mut s = String::with_capacity(64);
                            let mut both = false;
                            s.push('{');
                            if let Some(serial) = sdr.get("serial") {
                                s.push_str("\"sn\": \"");
                                s.push_str(serial);
                                s.push('\"');
                                both = true;
                            }
                            if let Some(label) = sdr.get("label") {
                                if both {
                                    s.push_str(", ");
                                }
                                s.push_str("\"label\": \"");
                                s.push_str(label);
                                s.push('\"');
                            }
                            s.push('}');
                            s
                        }).collect::<Vec<_>>().join(", \n"));
                        println!("}}");
                    }
                }
                Ok(())
            }
        }
        Err(e) => Err(match export_format {
            OutputFormat::Text => format!("Error listing SDRs: {:?}", e),
            OutputFormat::JSON => format!("{{\"err\": \"{:?}\"}}", e),
        })?,
    }
}