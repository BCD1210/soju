pub(super) const HELPERS: &str = r#"
inline float soju_ftz(float v) {
    uint b = as_type<uint>(v);
    return as_type<float>((b & 0x7f800000u) == 0u ? b & 0x80000000u : b);
}
inline float soju_fmz(float a, float b) {
    a = soju_ftz(a); b = soju_ftz(b);
    return a == 0.0f || b == 0.0f ? 0.0f : soju_ftz(a * b);
}
inline float soju_min(float a, float b) {
    if (a == 0.0f && b == 0.0f) return as_type<float>(as_type<uint>(a) | as_type<uint>(b));
    return fmin(a, b);
}
inline float soju_max(float a, float b) {
    if (a == 0.0f && b == 0.0f) return as_type<float>(as_type<uint>(a) & as_type<uint>(b));
    return fmax(a, b);
}
inline float soju_sat(float v) { return isnan(v) ? 0.0f : clamp(v, 0.0f, 1.0f); }
inline uint soju_lop3(uint a, uint b, uint c, uint lut) {
    uint out = 0u;
    for (uint i = 0; i < 8; ++i) {
        if ((lut >> i) & 1u) out |= ((i & 4u) ? a : ~a) & ((i & 2u) ? b : ~b) & ((i & 1u) ? c : ~c);
    }
    return out;
}
inline uint soju_bfe(uint a, uint control, bool sign) {
    uint start = control & 255u;
    uint width = (control >> 8u) & 255u;
    if (width == 0u) return 0u;
    if (start >= 32u) return sign && (a >> 31u) ? 0xffffffffu : 0u;
    width = min(width, 32u - start);
    uint v = a >> start;
    if (width == 32u) return v;
    uint mask = (1u << width) - 1u;
    v &= mask;
    if (sign && (v & (1u << (width - 1u)))) v |= ~mask;
    return v;
}
inline uint soju_bfi(uint insert, uint control, uint base) {
    uint start = control & 255u;
    uint width = (control >> 8u) & 255u;
    if (start >= 32u || width == 0u) return base;
    width = min(width, 32u - start);
    uint mask = width == 32u ? 0xffffffffu : ((1u << width) - 1u) << start;
    return (base & ~mask) | ((insert << start) & mask);
}
inline uint soju_f2i(float v, bool sign, uint mode) {
    v = mode == 0u ? rint(v) : mode == 1u ? floor(v) : mode == 2u ? ceil(v) : trunc(v);
    if (isnan(v)) return 0u;
    if (sign) {
        if (v >= 2147483648.0f) return 0x7fffffffu;
        if (v <= -2147483648.0f) return 0x80000000u;
        return as_type<uint>(int(v));
    }
    if (v >= 4294967296.0f) return 0xffffffffu;
    return v <= 0.0f ? 0u : uint(v);
}
"#;
