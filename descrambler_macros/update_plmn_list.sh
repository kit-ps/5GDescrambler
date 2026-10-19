#!/bin/sh
set -e

cd "$(dirname $0)" || exit 1
mkdir -p cache
curl -X GET 'https://mcc-mnc.com/api/v1/mcc-mnc.php' \
  -H 'Accept: application/json' > cache/plmn_list.json
