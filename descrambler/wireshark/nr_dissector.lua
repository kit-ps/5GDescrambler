-- SSB

local nr_ssb = Proto("nr-ssb", "NR SSB Protocol")

local f_sfn   = ProtoField.uint16("nr-ssb.sfn", "SFN", base.DEC)
local f_issb  = ProtoField.uint8("nr-ssb.i_ssb", "i_ssb", base.DEC)
local f_snr   = ProtoField.float("nr-ssb.snr", "SNR")

nr_ssb.fields = { f_sfn, f_issb, f_snr }

function nr_ssb.dissector(buffer, pinfo, tree)
    if buffer:len() < 13 then return end

    local magic = buffer(0,6):string()
    if magic ~= "nr-ssb" then return end

    pinfo.cols.protocol = "NR SSB"
    local i_ssb = buffer(8,1):uint()
    local snr_val = buffer(9,4):float()
    pinfo.cols.info = string.format("SSB (i_ssb = %d, SNR = %.2f dB)", i_ssb, snr_val)

    local subtree = tree:add(nr_ssb, buffer(), "NR SSB Information")
    subtree:add(f_sfn, buffer(6,2))
    subtree:add(f_issb, i_ssb)
    subtree:add(f_snr, snr_val):append_text(" dB")
end

DissectorTable.get("udp.port"):add(9998, nr_ssb)


-- DCI
local nr_dci = Proto("nr-dci", "NR DCI Protocol")

local f_sfn         = ProtoField.uint16("nr-dci.sfn", "SFN", base.DEC)
local f_symbol      = ProtoField.uint16("nr-dci.symbol", "Symbol", base.DEC)
local f_n_sym       = ProtoField.uint8("nr-dci.n_sym", "n_sym", base.DEC)
local f_al          = ProtoField.uint8("nr-dci.al", "AL", base.DEC)
local f_time_ns     = ProtoField.bytes("nr-dci.time_ns", "Time NS")
local f_errors      = ProtoField.uint16("nr-dci.errors", "Errors", base.DEC)
local f_free        = ProtoField.uint8("nr-dci.free", "Free", base.DEC)
local f_rnti        = ProtoField.uint16("nr-dci.rnti", "RNTI", base.DEC)
local f_n_id        = ProtoField.uint16("nr-dci.n_id", "N_ID", base.DEC)
local f_shared      = ProtoField.uint8("nr-dci.shared", "Shared", base.DEC)
local f_class       = ProtoField.uint8("nr-dci.classification", "Classification", base.DEC)
local f_data_len    = ProtoField.uint8("nr-dci.data_len", "Data Length", base.DEC)
local f_data_bits   = ProtoField.string("nr-dci.data_bits", "Data Bits")

nr_dci.fields = {
    f_sfn, f_symbol, f_n_sym, f_al, f_time_ns, f_errors,
    f_free, f_rnti, f_n_id, f_shared, f_class, f_data_len, f_data_bits
}

function nr_dci.dissector(buffer, pinfo, tree)
    if buffer:len() < 36 then return end -- Minimum header length

    local magic = buffer(0,6):string()
    if magic ~= "nr-dci" then return end

    pinfo.cols.protocol = "NR DCI"
    pinfo.cols.info = string.format("DCI (SFN=%d, Symbol=%d, n_sym=%d, al=%d, RNTI=%d, N_ID=%d)",
        buffer(6,2):uint(),
        buffer(8,2):uint(),
        buffer(10,1):uint(),
        buffer(11,1):uint(),
        buffer(31,2):uint(),
        buffer(33,2):uint()
    )

    local classification_map = {
        [0] = "Broadcast",
        [1] = "Likely Uplink",
        [2] = "Likely Downlink",
        [3] = "Definitely Uplink",
        [4] = "Definitely Downlink"
    }

    local time_bytes = buffer(12,16)

    local ns_total = 0
    local pow = 1
    for i = 15,0,-1 do
        ns_total = ns_total + time_bytes(i,1):uint() * pow
        pow = pow * 256
    end

    local ms_total = math.floor(ns_total / 1e6 + 0.5)
    local ms_str = tostring(ms_total)

    local subtree = tree:add(nr_dci, buffer(), "NR DCI Information")
    subtree:add(f_sfn, buffer(6,2))
    subtree:add(f_symbol, buffer(8,2))
    subtree:add(f_n_sym, buffer(10,1))
    subtree:add(f_al, buffer(11,1))
    subtree:add(f_time_ns, buffer(12,16)):append_text(" (ms: " .. ms_str .. ")")
    subtree:add(f_errors, buffer(28,2))
    subtree:add(f_free, buffer(30,1))
    subtree:add(f_rnti, buffer(31,2))
    subtree:add(f_n_id, buffer(33,2))
    subtree:add(f_shared, buffer(35,1)):append_text(" (" .. (buffer(35,1):uint() ~= 0 and "true" or "false") .. ")")
    subtree:add(f_class, buffer(36,1)):append_text(" (" .. (classification_map[buffer(36,1):uint()] or "Unknown") .. ")")
    subtree:add(f_data_len, buffer(37,1))

    local data_len = buffer(37,1):uint()
    local num_bytes = (data_len + 7) // 8

    if buffer:len() >= 38 + num_bytes then
        local payload = buffer(38, num_bytes)
        local bits = {}

        for i = 0, num_bytes - 1 do
            local b = payload(i,1):uint()
            for j = 7,0,-1 do
                if #bits < data_len then
                    table.insert(bits, (b >> j) & 1)
                end
            end
        end

        subtree:add(f_data_bits, tostring(table.concat(bits,", ")))
    end
end

DissectorTable.get("udp.port"):add(9997, nr_dci)