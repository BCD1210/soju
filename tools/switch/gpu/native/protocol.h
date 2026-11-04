#include <stdint.h>
#include <stddef.h>
typedef struct { const uint8_t *pointer; size_t length, capacity; } SpanByte;
typedef struct {
    int64_t address;
    int64_t value;
} Reg;
typedef struct { const Reg *pointer; size_t length, capacity; } SpanReg;
typedef struct {
    int64_t width;
    int64_t height;
    int64_t generation;
    int64_t metal;
    int64_t bytes;
    SpanByte key;
    SpanByte file;
} TargetCommand;
typedef struct { const TargetCommand *pointer; size_t length, capacity; } SpanTargetCommand;
typedef struct {
    int64_t format;
    int64_t generation;
    SpanByte key;
    SpanByte zfile;
    SpanByte sfile;
} DepthCommand;
typedef struct { const DepthCommand *pointer; size_t length, capacity; } SpanDepthCommand;
typedef struct {
    int64_t stage;
    int64_t slot;
    int64_t offset;
} ConstantCommand;
typedef struct { const ConstantCommand *pointer; size_t length, capacity; } SpanConstantCommand;
typedef struct {
    int64_t slot;
    int64_t stream;
    int64_t offset;
    int64_t format;
    int64_t stride;
    int64_t instanced;
    int64_t rate;
} AttributeCommand;
typedef struct { const AttributeCommand *pointer; size_t length, capacity; } SpanAttributeCommand;
typedef struct {
    int64_t slot;
    SpanByte file;
} BufferCommand;
typedef struct { const BufferCommand *pointer; size_t length, capacity; } SpanBufferCommand;
typedef struct {
    int64_t start;
    int64_t count;
} SegmentCommand;
typedef struct { const SegmentCommand *pointer; size_t length, capacity; } SpanSegmentCommand;
typedef struct {
    int64_t stage;
    int64_t count;
    int64_t image;
    int64_t sampler;
    int64_t width;
    int64_t height;
    int64_t metal;
    int64_t row;
    int64_t rows;
    int64_t u;
    int64_t v;
    int64_t min;
    int64_t mag;
    int64_t anisotropy;
    int64_t swizzle0;
    int64_t swizzle1;
    int64_t swizzle2;
    int64_t swizzle3;
    SpanByte key;
    SpanByte file;
} TextureCommand;
typedef struct { const TextureCommand *pointer; size_t length, capacity; } SpanTextureCommand;
typedef struct {
    int64_t reference;
    int64_t constant_vertex[32];
    int64_t constant_fragment[32];
    int64_t kind;
    int64_t repeats;
    int64_t clear;
    int64_t topology;
    int64_t first;
    int64_t count;
    int64_t index_size;
    int64_t base_vertex;
    int64_t instance;
    int64_t generation;
    int64_t slot;
    int64_t local0;
    int64_t local1;
    int64_t local2;
    int64_t groups0;
    int64_t groups1;
    int64_t groups2;
    SpanByte pipeline;
    SpanByte vertex;
    SpanByte fragment;
    SpanByte indices;
    SpanByte key;
    SpanByte source;
    SpanByte file;
    SpanReg registers;
    SpanTargetCommand targets;
    SpanDepthCommand depth;
    SpanConstantCommand constants;
    SpanAttributeCommand attributes;
    SpanBufferCommand buffers;
    SpanSegmentCommand segments;
    SpanTextureCommand textures;
} Command;
typedef struct { const Command *pointer; size_t length, capacity; } SpanCommand;
