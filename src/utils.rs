pub fn write_csv<I, R, W>(records: I, writer: W) -> color_eyre::Result<()>
where
    I: IntoIterator<Item = R>,
    R: serde::Serialize,
    W: std::io::Write,
{
    let mut wtr = csv::Writer::from_writer(writer);
    for record in records.into_iter() {
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}
