command -v python3 >/dev/null || {
  echo "::error::python3 missing; check_brand_strings would self-skip"
  exit 1
}
./script/check_brand_strings
