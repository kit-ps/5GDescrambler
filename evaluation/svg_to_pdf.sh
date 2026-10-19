#!/bin/sh

find . -type f -name "*.svg" -print | while IFS= read -r f
do
  echo "Converting $f..."
  inkscape "$f" --export-type=pdf --export-filename="${f%.svg}.pdf"
done
echo "Done."