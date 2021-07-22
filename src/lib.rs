pub mod benchmark;
pub mod config;
pub mod benchmarks;
pub mod zstd;
pub mod print;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
