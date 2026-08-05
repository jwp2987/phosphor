import struct, sys

def extract_authenticode(path):
    with open(path, 'rb') as f:
        data = f.read()
    if data[:2] != b'MZ':
        raise ValueError("not a PE file")
    pe_offset = struct.unpack_from('<I', data, 0x3C)[0]
    if data[pe_offset:pe_offset+4] != b'PE\x00\x00':
        raise ValueError("bad PE signature")
    opt_header_offset = pe_offset + 24
    magic = struct.unpack_from('<H', data, opt_header_offset)[0]
    is_pe32_plus = (magic == 0x20b)
    # Security directory is data directory index 4
    if is_pe32_plus:
        data_dir_offset = opt_header_offset + 112
    else:
        data_dir_offset = opt_header_offset + 96
    sec_dir_offset = data_dir_offset + 4 * 8
    sec_va, sec_size = struct.unpack_from('<II', data, sec_dir_offset)
    if sec_size == 0:
        return None
    # WIN_CERTIFICATE: dwLength(4) wRevision(2) wCertificateType(2) then cert data
    cert_blob = data[sec_va+8:sec_va+sec_size]
    return cert_blob

if __name__ == '__main__':
    for path in sys.argv[1:]:
        blob = extract_authenticode(path)
        if blob is None:
            print(f"{path}: NO AUTHENTICODE SIGNATURE FOUND")
        else:
            out = path + '.p7'
            with open(out, 'wb') as f:
                f.write(blob)
            print(f"{path}: signature blob extracted, {len(blob)} bytes -> {out}")
