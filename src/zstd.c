#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#include "common/cpu.h"
#include "common/huf.h"
#include "common/zstd_internal.h"
#include "compress/zstd_compress_internal.h"
#include "compress/zstd_compress_literals.h"
#include "decompress/zstd_decompress_block.h"
#include "decompress/zstd_decompress_internal.h"
#include "zstd.h"

#define PANIC() abort()
#define CONTROL(x)                                                             \
  do {                                                                         \
    if (!(x)) {                                                                \
      DEBUGLOG(3, "CONTROL %s failed", #x);                                    \
      PANIC();                                                                 \
    }                                                                          \
  } while (0)

// This file provides hooks into zstd that we may need for benchmarking and
// isn't convienent to do in rust.

typedef enum {
  ZSTD_BlockType_raw = 0,
  ZSTD_BlockType_rle = 1,
  ZSTD_BlockType_compressed = 2,
} ZSTD_BlockType_e;

typedef int (*ZSTD_BlockCallback_t)(void *opaque, void const *block,
                                    size_t blockSize, ZSTD_BlockType_e type);

size_t ZSTD_forEachBlock(void const *src, size_t srcSize,
                         ZSTD_BlockCallback_t callback, void *opaque);

typedef enum {
  ZSTD_LiteralsBlockType_raw = 0,
  ZSTD_LiteralsBlockType_rle = 1,
  ZSTD_LiteralsBlockType_compressed = 2,
  ZSTD_LiteralsBlockType_repeat = 3,
} ZSTD_LiteralsBlockType_e;

size_t ZSTD_getLiteralsFromBlock(void const **cLiterals,
                                 ZSTD_LiteralsBlockType_e *type,
                                 void const *src, size_t srcSize);

typedef int (*ZSTD_LiteralsBlockCallback_t)(void *opaque, void const *cLiterals,
                                            size_t cLiteralsSize,
                                            void const *dLiterals,
                                            size_t dLiteralsSize,
                                            ZSTD_LiteralsBlockType_e type);

size_t ZSTD_forEachLiteralsBlock(void const *src, size_t srcSize,
                                 ZSTD_LiteralsBlockCallback_t callback,
                                 void *opaque);

size_t ZSTD_decodeAllLiteralBlocks(void const *src, size_t srcSize);

void *ZSTD_CompressLiteralsBlockContext_create(void);
void ZSTD_CompressLiteralsBlockContext_free(void *ctx);
size_t ZSTD_compressLiteralsBlock(void *ctx, void const *src, size_t srcSize,
                                  int suspectUncompressible);

static ZSTD_BlockType_e ZSTD_BlockType_map(blockType_e type) {
  switch (type) {
  case bt_raw:
    return ZSTD_BlockType_raw;
  case bt_rle:
    return ZSTD_BlockType_rle;
  case bt_compressed:
    return ZSTD_BlockType_compressed;
  case bt_reserved:
  default:
    PANIC();
  }
}

size_t ZSTD_forEachBlock(void const *src, size_t srcSize,
                         ZSTD_BlockCallback_t callback, void *opaque) {
  uint8_t const *ip = src;
  uint8_t const *const iend = ip + srcSize;
  size_t const fhs = ZSTD_frameHeaderSize(src, srcSize);
  FORWARD_IF_ERROR(fhs, "corrupt zstd frame");
  ip += fhs;

  blockProperties_t props;
  size_t blocks = 0;
  for (;;) {
    size_t const remaining = (size_t)(iend - ip);
    size_t const csize = ZSTD_getcBlockSize(ip, remaining, &props);
    FORWARD_IF_ERROR(csize, "corrupt zstd block");
    RETURN_ERROR_IF(csize + ZSTD_blockHeaderSize > remaining, srcSize_wrong,
                    "src size too small");

    if (callback(opaque, ip, ZSTD_blockHeaderSize + csize,
                 ZSTD_BlockType_map(props.blockType)))
      break;

    ip += ZSTD_blockHeaderSize + csize;
    ++blocks;

    if (props.lastBlock)
      break;
  }

  return blocks;
}
ZSTD_LiteralsBlockType_e
ZSTD_LiteralsBlockType_map(symbolEncodingType_e litEncType) {
  switch (litEncType) {
  case set_repeat:
    return ZSTD_LiteralsBlockType_repeat;
  case set_compressed:
    return ZSTD_LiteralsBlockType_compressed;
  case set_basic:
    return ZSTD_LiteralsBlockType_raw;
  case set_rle:
    return ZSTD_LiteralsBlockType_rle;
  default:
    PANIC();
  }
}

size_t ZSTD_getLiteralsFromBlock(void const **cLiterals,
                                 ZSTD_LiteralsBlockType_e *type,
                                 void const *src, size_t srcSize) {
  uint8_t const *ip = src;
  blockProperties_t props;
  size_t const csize = ZSTD_getcBlockSize(ip, srcSize, &props);
  FORWARD_IF_ERROR(csize, "corrupt zstd block");
  RETURN_ERROR_IF(props.blockType != bt_compressed, GENERIC,
                  "block is not compressed");
  RETURN_ERROR_IF(csize + ZSTD_blockHeaderSize > srcSize, srcSize_wrong,
                  "src size too small");
  ip += ZSTD_blockHeaderSize;
  RETURN_ERROR_IF(csize < 3, corruption_detected, "block too small");
  symbolEncodingType_e const litEncType = (symbolEncodingType_e)(ip[0] & 3);

  *cLiterals = ip;
  *type = ZSTD_LiteralsBlockType_map(litEncType);

  size_t litHSize, litCSize;
  switch (litEncType) {
  case set_repeat:
  case set_compressed:
    RETURN_ERROR_IF(csize < 5, corruption_detected, "lb on block size is 5");
    {
      uint32_t const lhlCode = (ip[0] >> 2) & 3;
      uint32_t const lhc = MEM_readLE32(ip);
      switch (lhlCode) {
      case 0:
      case 1:
      default:
        litHSize = 3;
        litCSize = (lhc >> 14) & 0x3FF;
        break;
      case 2:
        litHSize = 4;
        litCSize = lhc >> 18;
        break;
      case 3:
        litHSize = 5;
        litCSize = (lhc >> 22) + ((size_t)ip[4] << 10);
        break;
      }
      break;
    }
  case set_basic:
  case set_rle:
    switch ((ip[0] >> 2) & 3) {
    case 0:
    case 2:
    default:
      litHSize = 1;
      litCSize = ip[0] >> 3;
      break;
    case 1:
      litHSize = 2;
      litCSize = MEM_readLE16(ip) >> 4;
      break;
    case 3:
      litHSize = 3;
      litCSize = MEM_readLE24(ip) >> 4;
      break;
    }
    break;
   default: PANIC();
  }
  size_t const litSize = litHSize + (litEncType == set_rle ? 1 : litCSize);
  RETURN_ERROR_IF(litSize > csize, corruption_detected, "lits too large");
  return litSize;
}

typedef struct {
  ZSTD_DCtx *dctx;
  size_t error;
  ZSTD_LiteralsBlockCallback_t callback;
  void *opaque;
} ZSTD_ForEachLiteralsBlock_Data;

size_t ZSTD_decodeLiteralsBlock(ZSTD_DCtx *dctx, const void *src,
                                size_t srcSize);

int ZSTD_forEachLiteralsBlock_callback(void *opaque, void const *block,
                                       size_t blockSize,
                                       ZSTD_BlockType_e type) {
  uint8_t const* const lits = (uint8_t const*)block + ZSTD_blockHeaderSize;
  size_t const litsSize = blockSize - ZSTD_blockHeaderSize;
  ZSTD_ForEachLiteralsBlock_Data *data =
      (ZSTD_ForEachLiteralsBlock_Data *)opaque;
  if (type != ZSTD_BlockType_compressed)
    return 0;
  size_t const csize = ZSTD_decodeLiteralsBlock(data->dctx, lits, litsSize);
  if (ZSTD_isError(csize)) {
    data->error = csize;
    return 1;
  }
  ZSTD_LiteralsBlockType_e literalsType;
  void const *cLiterals;
  size_t const cLiteralsSize =
      ZSTD_getLiteralsFromBlock(&cLiterals, &literalsType, block, blockSize);
  if (ZSTD_isError(cLiteralsSize)) {
    data->error = cLiteralsSize;
    return 1;
  }

  CONTROL(cLiteralsSize == csize);
  return data->callback(data->opaque, cLiterals, cLiteralsSize,
                        data->dctx->litPtr, data->dctx->litSize, literalsType);
}

size_t ZSTD_forEachLiteralsBlock(void const *src, size_t srcSize,
                                 ZSTD_LiteralsBlockCallback_t callback,
                                 void *opaque) {
  ZSTD_ForEachLiteralsBlock_Data data = {
      .dctx = ZSTD_createDCtx(),
      .error = 0,
      .callback = callback,
      .opaque = opaque,
  };
  RETURN_ERROR_IF(data.dctx == NULL, memory_allocation, "OOM");
  FORWARD_IF_ERROR(ZSTD_decompressBegin(data.dctx), "decompress begin failed");
  size_t const blocks = ZSTD_forEachBlock(
      src, srcSize, ZSTD_forEachLiteralsBlock_callback, &data);
  ZSTD_freeDCtx(data.dctx);
  FORWARD_IF_ERROR(blocks, "for each block error");
  FORWARD_IF_ERROR(data.error, "literals error");
  return blocks;
}

typedef struct {
  ZSTD_hufCTables_t prev;
  ZSTD_hufCTables_t next;
  uint8_t dst[128 * 1024];
  uint64_t workspace[2048];
  int bmi2;
} ZSTD_CompressLiteralsBlockContext;

void *ZSTD_CompressLiteralsBlockContext_create(void) {
  ZSTD_CompressLiteralsBlockContext *ctx =
      (ZSTD_CompressLiteralsBlockContext *)malloc(
          sizeof(ZSTD_CompressLiteralsBlockContext));
  if (ctx == NULL)
    return ctx;
  ctx->prev.repeatMode = HUF_repeat_none;
  ctx->bmi2 = ZSTD_cpuid_bmi2(ZSTD_cpuid());
  return ctx;
}

void ZSTD_CompressLiteralsBlockContext_free(void *ctx) { free(ctx); }

size_t ZSTD_compressLiteralsBlock(void *opaque, void const *src, size_t srcSize,
                                  int suspectUncompressible) {
  ZSTD_CompressLiteralsBlockContext *ctx =
      (ZSTD_CompressLiteralsBlockContext *)opaque;
  RETURN_ERROR_IF(srcSize > sizeof(ctx->dst), srcSize_wrong, "too many lits");
#if ZSTD_VERSION_NUMBER >= 10500
  return ZSTD_compressLiterals(
      &ctx->prev, &ctx->next, ZSTD_fast, /* disableLiteralCompression */ 0,
      ctx->dst, sizeof(ctx->dst), src, srcSize, ctx->workspace,
      sizeof(ctx->workspace), ctx->bmi2, suspectUncompressible);
#else
  (void)suspectUncompressible;
  return ZSTD_compressLiterals(&ctx->prev, &ctx->next, ZSTD_fast,
                               /* disableLiteralCompression */ 0, ctx->dst,
                               sizeof(ctx->dst), src, srcSize, ctx->workspace,
                               sizeof(ctx->workspace), ctx->bmi2);
#endif
}

size_t ZSTD_decodeLiteralsBlock(ZSTD_DCtx* dctx, const void* src, size_t srcSize);

size_t ZSTD_decompressLiteralsBlock(ZSTD_DCtx* dctx, const void* src, size_t srcSize) {
  size_t const csize = ZSTD_decodeLiteralsBlock(dctx, src, srcSize);
  FORWARD_IF_ERROR(csize, "decode literals block failed");
  return dctx->litSize;
}

size_t HUF_sizeofCTableU64(size_t maxSymbol) {
  return HUF_CTABLE_SIZE_U32(maxSymbol) / 2 + 1;
}
size_t HUF_sizeofDTableU32(size_t maxTableLog) {
  return (HUF_DTABLE_SIZE(maxTableLog) * (sizeof(HUF_DTable))) / sizeof(U32) + 1;
}
size_t HUF_sizeofWorkspaceU32() {
  return HUF_WORKSPACE_SIZE_U32;
}
int ZSTD_hasBMI2() {
  return ZSTD_cpuid_bmi2(ZSTD_cpuid());
}
