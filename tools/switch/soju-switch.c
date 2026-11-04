/*
 * Experimental Switch homebrew execution research.
 * A bounded interpreter; guest instructions never execute as host code.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_IMAGE (64u * 1024u * 1024u)
#define MAX_STEPS 1000000u

static uint32_t u32(const unsigned char *p) {
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 |
           (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}
static int error(const char *message) {
    fprintf(stderr, "soju-switch: %s\n", message);
    return 1;
}
static int run(const unsigned char *data, size_t length) {
    if (length < 0x80 || memcmp(data + 0x10, "NRO0", 4))
        return error("invalid or truncated NRO header");
    uint32_t version = u32(data + 0x14), size = u32(data + 0x18);
    if (version != 0) return error("unsupported NRO version");
    if (size < 0x80 || size > length || size > MAX_IMAGE)
        return error("invalid NRO image size");
    uint32_t off[3], len[3];
    for (int i = 0; i < 3; ++i) {
        off[i] = u32(data + 0x20 + i * 8);
        len[i] = u32(data + 0x24 + i * 8);
        if (off[i] > size || len[i] > size - off[i])
            return error("NRO segment outside image");
        if (i && off[i] < off[i-1] + len[i-1])
            return error("overlapping or unordered NRO segments");
    }
    if (off[0] != 0 || len[0] < 0x80 || (len[0] & 3))
        return error("invalid executable segment");
    /* x31 denotes the zero register for the instructions implemented here. */
    uint64_t x[31] = {0};
    unsigned n = 0, z = 0, c = 0, v = 0;
    uint64_t pc = 0;
    for (unsigned step = 0; step < MAX_STEPS; ++step) {
        if ((pc & 3) || pc > len[0] || len[0] - pc < 4)
            return error("instruction fetch outside executable segment");
        uint32_t ins = u32(data + pc);
        uint64_t here = pc;
        pc += 4;
        if (ins == 0xd503201f) continue; /* NOP */
        if ((ins & 0x7f80001fu) == 0x7100001fu) { /* CMP immediate */
            unsigned rn = (ins >> 5) & 31u;
            if (rn == 31) return error("CMP with SP is not implemented");
            unsigned bits = (ins >> 31) ? 64 : 32;
            uint64_t mask = bits == 64 ? UINT64_MAX : UINT32_MAX;
            uint64_t left = x[rn] & mask;
            uint64_t right = ((ins >> 10) & 0xfffu) << (((ins >> 22) & 1u) ? 12 : 0);
            uint64_t result = (left - right) & mask;
            uint64_t sign = UINT64_C(1) << (bits - 1);
            n = (result & sign) != 0; z = result == 0; c = left >= right;
            v = ((left ^ right) & (left ^ result) & sign) != 0;
            continue;
        }
        if ((ins & 0xff000010u) == 0x54000000u) { /* B.cond */
            unsigned cond = ins & 15u, take;
            switch (cond >> 1) {
                case 0: take = z; break; case 1: take = c; break;
                case 2: take = n; break; case 3: take = v; break;
                case 4: take = c && !z; break; case 5: take = n == v; break;
                case 6: take = !z && (n == v); break; default: take = 1; break;
            }
            if ((cond & 1u) && cond != 15) take = !take;
            if (take) {
                int64_t imm = (ins >> 5) & 0x7ffffu;
                if (imm & 0x40000) imm -= 0x80000;
                int64_t target = (int64_t)here + imm * 4;
                if (target < 0 || (uint64_t)target >= len[0])
                    return error("branch outside executable segment");
                pc = (uint64_t)target;
            }
            continue;
        }
        if ((ins & 0xfc000000u) == 0x14000000u) { /* B imm26 */
            int64_t imm = ins & 0x03ffffffu;
            if (imm & 0x02000000) imm -= 0x04000000;
            int64_t target = (int64_t)here + imm * 4;
            if (target < 0 || (uint64_t)target >= len[0])
                return error("branch outside executable segment");
            pc = (uint64_t)target;
            continue;
        }
        if ((ins & 0x7f800000u) == 0x52800000u) { /* MOVZ Wd/Xd */
            unsigned rd = ins & 31u, shift = ((ins >> 21) & 3u) * 16;
            int is64 = (ins >> 31) != 0;
            if (!is64 && shift >= 32) return error("invalid MOVZ encoding");
            if (rd != 31) x[rd] = (uint64_t)((ins >> 5) & 0xffffu) << shift;
            continue;
        }
        if ((ins & 0xffe0001fu) == 0xd4000001u) { /* SVC imm16 */
            unsigned svc = (ins >> 5) & 0xffffu;
            if (svc == 7) {
                printf("soju-switch: guest ExitProcess (x0=0x%llx, steps=%u)\n",
                       (unsigned long long)x[0], step + 1);
                return 0;
            }
            fprintf(stderr, "soju-switch: unsupported SVC 0x%x at 0x%llx\n",
                    svc, (unsigned long long)here);
            return 1;
        }
        fprintf(stderr, "soju-switch: unsupported instruction 0x%08x at 0x%llx\n",
                ins, (unsigned long long)here);
        return 1;
    }
    return error("instruction limit exceeded");
}
int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: soju-switch <experimental-homebrew.nro>\n");
        return 2;
    }
    FILE *f = fopen(argv[1], "rb");
    if (!f) return error("cannot open input");
    if (fseek(f, 0, SEEK_END) || ftell(f) < 0) { fclose(f); return error("cannot size input"); }
    long size = ftell(f);
    /* Keep resource use bounded, including any appended NRO assets. */
    if ((unsigned long)size > MAX_IMAGE) { fclose(f); return error("input exceeds 64 MiB limit"); }
    rewind(f);
    unsigned char *data = malloc(size ? (size_t)size : 1);
    if (!data) { fclose(f); return error("out of memory"); }
    size_t got = fread(data, 1, (size_t)size, f);
    int close_result = fclose(f);
    if (got != (size_t)size || close_result) { free(data); return error("cannot read input"); }
    int result = run(data, got);
    free(data);
    return result;
}
