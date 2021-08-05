extern crate libc;
#[cfg(zstd)]
use libc::{c_void, size_t};

#[cfg(zstd)]
#[link(name = "zstd_bench")]
extern "C" {
	fn ZSTD_isError(result: size_t) -> i32;
	fn ZSTD_compress(
		dst: *mut c_void,
		dstCapacity: size_t,
		src: *const c_void,
		srcSize: size_t,
		level: i32,
	) -> size_t;
	fn ZSTD_compressBound(srcSize: size_t) -> size_t;

	fn ZSTD_CompressLiteralsBlockContext_create() -> *mut c_void;
	fn ZSTD_CompressLiteralsBlockContext_free(ctx: *mut c_void);

	fn ZSTD_compressLiteralsBlock(
		ctx: *mut c_void,
		src: *const c_void,
		srcSize: size_t,
		suspectUncompressible: i32,
	) -> size_t;

	fn ZSTD_createDCtx() -> *mut c_void;
	fn ZSTD_decompressBegin(dctx: *mut c_void) -> size_t;
	fn ZSTD_freeDCtx(dctx: *mut c_void);

	fn ZSTD_decompressLiteralsBlock(
		dctx: *mut c_void,
		src: *const c_void,
		srcSize: size_t,
	) -> size_t;

	fn ZSTD_forEachBlock(
		src: *const c_void,
		srcSize: size_t,
		callback: extern "C" fn(*mut c_void, *const u8, size_t, BlockType) -> i32,
		opaque: *mut c_void,
	) -> size_t;

	fn ZSTD_forEachLiteralsBlock(
		src: *const c_void,
		srcSize: size_t,
		callback: extern "C" fn(
			*mut c_void,
			*const u8,
			size_t,
			*const u8,
			size_t,
			LiteralsBlockType,
		) -> i32,
		opaque: *mut c_void,
	) -> size_t;

	fn HUF_sizeofCTableU64(maxSymbol: size_t) -> size_t;
	fn HUF_sizeofDTableU32(maxTableLog: size_t) -> size_t;
	fn HUF_sizeofWorkspaceU32() -> size_t;

	fn HUF_buildCTable_wksp(
		ctable: *mut c_void,
		count: *const u32,
		max_symbol: u32,
		max_table_log: u32,
		wksp: *mut c_void,
		wksp_size: size_t,
	) -> size_t;
	fn HUF_writeCTable_wksp(
		dst: *mut c_void,
		dst_capacity: size_t,
		ctable: *const c_void,
		max_symbol: u32,
		table_log: u32,
		wksp: *mut c_void,
		wksp_size: size_t,
	) -> size_t;
	fn HUF_compress1X_usingCTable(
		dst: *mut c_void,
		dst_capacity: size_t,
		src: *const c_void,
		src_size: size_t,
		ctable: *const c_void,
	) -> size_t;
	fn HUF_compress4X_usingCTable(
		dst: *mut c_void,
		dst_capacity: size_t,
		src: *const c_void,
		src_size: size_t,
		ctable: *const c_void,
	) -> size_t;
	fn HUF_readDTableX1_wksp_bmi2(
		dtable: *mut c_void,
		src: *const c_void,
		src_size: size_t,
		wksp: *mut c_void,
		wksp_size: size_t,
		bmi2: i32,
	) -> size_t;
	fn HUF_readDTableX2_wksp(
		dtable: *mut c_void,
		src: *const c_void,
		src_size: size_t,
		wksp: *mut c_void,
		wksp_size: size_t,
	) -> size_t;
	fn HUF_decompress1X_usingDTable_bmi2(
		dst: *mut c_void,
		dst_capacity: size_t,
		src: *const c_void,
		src_size: size_t,
		dtable: *const c_void,
		bmi2: i32,
	) -> size_t;
	fn HUF_decompress4X_usingDTable_bmi2(
		dst: *mut c_void,
		dst_capacity: size_t,
		src: *const c_void,
		src_size: size_t,
		dtable: *const c_void,
		bmi2: i32,
	) -> size_t;

	fn ZSTD_hasBMI2() -> i32;
}

pub enum IterationCommand {
	Break,
	Continue,
}

#[repr(i32)]
#[derive(Debug, PartialEq, Eq)]
pub enum BlockType {
	Raw = 0,
	Rle = 1,
	Compressed = 2,
}

#[repr(i32)]
#[derive(Debug, PartialEq, Eq)]
pub enum LiteralsBlockType {
	Raw = 0,
	Rle = 1,
	Compressed = 2,
	Repeat = 3,
}

#[derive(Clone, Copy)]
pub enum HufStreams {
	SingleStream,
	FourStreams,
}

#[derive(Clone, Copy)]
pub enum HufDecompressMode {
	SingleSymbol,
	DoubleSymbol,
}

#[cfg(zstd)]
mod zstd_enabled {
	use super::*;
	use std::mem::transmute;

	pub fn compress_bound(src_size: usize) -> usize {
		unsafe { ZSTD_compressBound(src_size) }
	}

	pub fn is_error(result: usize) -> bool {
		unsafe { ZSTD_isError(result) != 0 }
	}

	pub fn compress(dst: &mut [u8], src: &[u8], level: i32) -> usize {
		unsafe {
			ZSTD_compress(
				dst.as_mut_ptr() as *mut c_void,
				dst.len(),
				src.as_ptr() as *const c_void,
				src.len(),
				level,
			)
		}
	}

	struct ForEachBlockData<'a> {
		callback: &'a mut dyn FnMut(&[u8], BlockType) -> IterationCommand,
	}

	extern "C" fn for_each_block_callback<'a>(
		opaque: *mut c_void,
		block_ptr: *const u8,
		block_size: size_t,
		block_type: BlockType,
	) -> i32 {
		let cmd = unsafe {
			let data = transmute::<*mut c_void, *mut ForEachBlockData<'a>>(opaque);
			let block = std::slice::from_raw_parts(block_ptr, block_size);
			((*data).callback)(block, block_type)
		};
		match cmd {
			IterationCommand::Break => 1,
			IterationCommand::Continue => 0,
		}
	}

	pub fn for_each_block(
		frame: &[u8],
		callback: impl FnMut(&[u8], BlockType) -> IterationCommand,
	) -> usize {
		let mut callback = callback;
		let mut data = ForEachBlockData {
			callback: &mut callback,
		};
		unsafe {
			ZSTD_forEachBlock(
				frame.as_ptr() as *const c_void,
				frame.len(),
				for_each_block_callback,
				&mut data as *mut _ as *mut _,
			)
		}
	}

	struct ForEachLiteralsBlockData<'a> {
		callback: &'a mut dyn FnMut(&[u8], &[u8], LiteralsBlockType) -> IterationCommand,
	}

	extern "C" fn for_each_literals_block_callback<'a>(
		opaque: *mut c_void,
		c_literals_ptr: *const u8,
		c_literals_size: size_t,
		d_literals_ptr: *const u8,
		d_literals_size: size_t,
		literals_type: LiteralsBlockType,
	) -> i32 {
		let cmd = unsafe {
			let data =
				transmute::<*mut c_void, *mut ForEachLiteralsBlockData<'a>>(opaque);
			let c_literals =
				std::slice::from_raw_parts(c_literals_ptr, c_literals_size);
			let d_literals =
				std::slice::from_raw_parts(d_literals_ptr, d_literals_size);
			((*data).callback)(c_literals, d_literals, literals_type)
		};
		match cmd {
			IterationCommand::Break => 1,
			IterationCommand::Continue => 0,
		}
	}

	pub fn for_each_literals_block(
		frame: &[u8],
		callback: impl FnMut(&[u8], &[u8], LiteralsBlockType) -> IterationCommand,
	) -> usize {
		let mut callback = callback;
		let mut data = ForEachLiteralsBlockData {
			callback: &mut callback,
		};
		unsafe {
			ZSTD_forEachLiteralsBlock(
				frame.as_ptr() as *const c_void,
				frame.len(),
				for_each_literals_block_callback,
				&mut data as *mut _ as *mut _,
			)
		}
	}

	pub struct LiteralsBlockCompressor {
		ctx: *mut c_void,
	}

	impl LiteralsBlockCompressor {
		pub fn new() -> Self {
			let ctx = unsafe { ZSTD_CompressLiteralsBlockContext_create() };
			assert_eq!(ctx.is_null(), false);
			LiteralsBlockCompressor { ctx }
		}

		pub fn compress(&mut self, literals: &[u8]) -> usize {
			unsafe {
				ZSTD_compressLiteralsBlock(
					self.ctx,
					literals.as_ptr() as *const c_void,
					literals.len(),
					0,
				)
			}
		}
	}

	impl Drop for LiteralsBlockCompressor {
		fn drop(&mut self) {
			unsafe {
				ZSTD_CompressLiteralsBlockContext_free(self.ctx);
			}
			self.ctx = std::ptr::null_mut();
		}
	}

	pub struct LiteralsBlockDecompressor {
		dctx: *mut c_void,
	}

	impl LiteralsBlockDecompressor {
		pub fn new() -> Self {
			let dctx = unsafe {
				let dctx = ZSTD_createDCtx();
				assert_eq!(dctx.is_null(), false);
				let ret = ZSTD_decompressBegin(dctx);
				assert_eq!(ZSTD_isError(ret), 0);
				dctx
			};

			LiteralsBlockDecompressor { dctx }
		}

		pub fn decompress(&mut self, literals: &[u8]) -> usize {
			unsafe {
				let dsize = ZSTD_decompressLiteralsBlock(
					self.dctx,
					literals.as_ptr() as *const c_void,
					literals.len(),
				);
				assert_eq!(ZSTD_isError(dsize), 0);
				dsize
			}
		}
	}

	impl Drop for LiteralsBlockDecompressor {
		fn drop(&mut self) {
			unsafe {
				ZSTD_freeDCtx(self.dctx);
			}
			self.dctx = std::ptr::null_mut();
		}
	}

	pub struct Huffman {
		wksp: Vec<u32>,
		wksp_bytes: usize,
		bmi2: i32,
	}

	pub struct HufCTable {
		ctable: Vec<u64>,
		table_log: usize,
		max_symbol: u8,
		max_table_log: usize,
	}

	impl Huffman {
		pub fn new() -> Self {
			let mut wksp = Vec::new();
			let size = unsafe { HUF_sizeofWorkspaceU32() };
			wksp.resize(size, 0);
			Huffman {
				wksp,
				wksp_bytes: size * 4,
				bmi2: unsafe { ZSTD_hasBMI2() },
			}
		}

		pub fn new_ctable(
			max_symbol: Option<u8>,
			max_table_log: Option<usize>,
		) -> HufCTable {
			let mut ctable = Vec::new();
			let size = unsafe { HUF_sizeofCTableU64(max_symbol.unwrap_or(255).into()) };
			ctable.resize(size, 0);
			HufCTable {
				ctable,
				max_symbol: max_symbol.unwrap_or(255),
				table_log: 0,
				max_table_log: max_table_log.unwrap_or(11),
			}
		}

		pub fn new_dtable(max_table_log: Option<usize>) -> Vec<u32> {
			let mut dtable = Vec::new();
			let size = unsafe { HUF_sizeofDTableU32(max_table_log.unwrap_or(12)) };
			dtable.resize(size, 0);
			dtable[0] = (max_table_log.unwrap_or(12) as u32) * 0x01000001;
			dtable
		}

		pub fn build_ctable(&mut self, ctable: &mut HufCTable, count: &[u32]) {
			assert_ne!(count.len(), 0);
			ctable.max_symbol = (count.len() - 1) as u8;
			let table_log = unsafe {
				HUF_buildCTable_wksp(
					ctable.ctable.as_mut_ptr() as _,
					count.as_ptr(),
					ctable.max_symbol.into(),
					ctable.max_table_log as u32,
					self.wksp.as_mut_ptr() as _,
					self.wksp_bytes,
				)
			};
			assert_eq!(is_error(table_log), false);
			ctable.table_log = table_log;
		}

		pub fn write_ctable(&mut self, dst: &mut [u8], ctable: &HufCTable) -> usize {
			assert_ne!(ctable.table_log, 0);
			let dst_size = unsafe {
				HUF_writeCTable_wksp(
					dst.as_mut_ptr() as _,
					dst.len(),
					ctable.ctable.as_ptr() as _,
					ctable.max_symbol.into(),
					ctable.table_log as u32,
					self.wksp.as_mut_ptr() as _,
					self.wksp_bytes,
				)
			};
			assert_eq!(is_error(dst_size), false);
			dst_size
		}

		pub fn compress(
			&mut self,
			dst: &mut [u8],
			src: &[u8],
			ctable: &HufCTable,
			streams: HufStreams,
		) -> usize {
			assert_ne!(ctable.table_log, 0);
			let compress = match streams {
				HufStreams::SingleStream => HUF_compress1X_usingCTable,
				HufStreams::FourStreams => HUF_compress4X_usingCTable,
			};
			let dst_size = unsafe {
				compress(
					dst.as_mut_ptr() as _,
					dst.len(),
					src.as_ptr() as _,
					src.len(),
					ctable.ctable.as_ptr() as _,
				)
			};
			assert_eq!(is_error(dst_size), false);
			dst_size
		}

		pub fn read_dtable(
			&mut self,
			src: &[u8],
			dtable: &mut [u32],
			mode: HufDecompressMode,
		) -> usize {
			let read = unsafe {
				match mode {
					HufDecompressMode::SingleSymbol => {
						HUF_readDTableX1_wksp_bmi2(
							dtable.as_mut_ptr() as _,
							src.as_ptr() as _,
							src.len(),
							self.wksp.as_mut_ptr() as _,
							self.wksp_bytes,
							self.bmi2,
						)
					}
					HufDecompressMode::DoubleSymbol => HUF_readDTableX2_wksp(
						dtable.as_mut_ptr() as _,
						src.as_ptr() as _,
						src.len(),
						self.wksp.as_mut_ptr() as _,
						self.wksp_bytes,
					),
				}
			};
			assert_eq!(is_error(read), false);
			read
		}

		pub fn decompress(
			&mut self,
			dst: &mut [u8],
			src: &[u8],
			dtable: &[u32],
			streams: HufStreams,
		) -> usize {
			let decompress = match streams {
				HufStreams::SingleStream => HUF_decompress1X_usingDTable_bmi2,
				HufStreams::FourStreams => HUF_decompress4X_usingDTable_bmi2,
			};
			let dst_size = unsafe {
				decompress(
					dst.as_mut_ptr() as _,
					dst.len(),
					src.as_ptr() as _,
					src.len(),
					dtable.as_ptr() as _,
					self.bmi2,
				)
			};
			assert_eq!(is_error(dst_size), false);
			dst_size
		}
	}
}

#[cfg(not(zstd))]
mod zstd_disabled {
	use super::*;

	pub fn compress_bound(_src_size: usize) -> usize {
		0
	}

	pub fn is_error(_result: usize) -> bool {
		false
	}

	pub fn compress(_dst: &mut [u8], _src: &[u8], _level: i32) -> usize {
		0
	}

	pub fn for_each_block(
		_frame: &[u8],
		_callback: impl FnMut(&[u8], BlockType) -> IterationCommand,
	) -> usize {
		0
	}

	pub fn for_each_literals_block(
		_frame: &[u8],
		_callback: impl FnMut(&[u8], &[u8], LiteralsBlockType) -> IterationCommand,
	) -> usize {
		0
	}

	pub struct LiteralsBlockCompressor {}

	impl LiteralsBlockCompressor {
		pub fn new() -> Self {
			LiteralsBlockCompressor {}
		}

		pub fn compress(&mut self, _literals: &[u8]) -> usize {
			0
		}
	}

	pub struct LiteralsBlockDecompressor {}

	impl LiteralsBlockDecompressor {
		pub fn new() -> Self {
			LiteralsBlockDecompressor {}
		}

		pub fn decompress(&mut self, _literals: &[u8]) -> usize {
			0
		}
	}

	pub struct Huffman {}

	pub struct HufCTable {}

	impl Huffman {
		pub fn new() -> Self {
			Huffman {}
		}

		pub fn new_ctable(
			_max_symbol: Option<u8>,
			_max_table_log: Option<usize>,
		) -> HufCTable {
			HufCTable {}
		}

		pub fn new_dtable(_max_table_log: Option<u32>) -> Vec<u32> {
			Vec::new()
		}

		pub fn build_ctable(&mut self, _ctable: &mut HufCTable, _count: &[u32]) {}

		pub fn write_ctable(&mut self, _dst: &mut [u8], _ctable: &HufCTable) -> usize {
			0
		}

		pub fn compress(
			&mut self,
			_dst: &mut [u8],
			_src: &[u8],
			_ctable: &HufCTable,
			_streams: HufStreams,
		) -> usize {
			0
		}

		pub fn read_dtable(
			&mut self,
			_src: &[u8],
			_dtable: &mut [u32],
			_mode: HufDecompressMode,
		) -> usize {
			0
		}

		pub fn decompress(
			&mut self,
			_dst: &mut [u8],
			_src: &[u8],
			_dtable: &[u32],
			_streams: HufStreams,
		) -> usize {
			0
		}
	}
}

#[cfg(zstd)]
pub use zstd_enabled::*;

#[cfg(not(zstd))]
pub use zstd_disabled::*;
