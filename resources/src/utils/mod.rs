use anyhow::Result;

pub fn first_error_or_ok<T>(results: Vec<Result<T>>) -> Result<()> {
    results
        .into_iter()
        .find_map(|r| r.err())
        .map_or(Ok(()), Err)
}
