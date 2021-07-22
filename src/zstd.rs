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
}

pub enum IterationCommand {
	Break,
	Continue,
}

#[repr(i32)]
#[derive(Debug)]
pub enum BlockType {
	Raw = 0,
	Rle = 1,
	Compressed = 2,
}

#[repr(i32)]
pub enum LiteralsBlockType {
	Raw = 0,
	Rle = 1,
	Compressed = 2,
	Repeat = 3,
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
}

#[cfg(zstd)]
pub use zstd_enabled::*;

#[cfg(not(zstd))]
pub use zstd_disabled::*;
