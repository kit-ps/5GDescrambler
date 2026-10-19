This crate automatically compiles NR RRC definitions from ASN1 files.
ASN1 files were created for release 19 by using the OpenAirInterface5G script
from [here](https://gitlab.eurecom.fr/oai/openairinterface5g/-/blob/293c6551f24559a0817ed680b2520dba7f2cf60d/openair2/RRC/NR/MESSAGES/ASN.1/extract_asn1_from_spce.pl).

Changes applied:

- Reduced the full list of definitions to only the definitions required for MIB and SIB1, without extensions.
- From the definition of MIB the systemFrameNumber was removed due to us parsing this value in a pre-processing step.