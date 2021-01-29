pub fn write_csv<R, W>(records: Vec<R>, writer: W) -> color_eyre::Result<()>
where
    R: serde::Serialize,
    W: std::io::Write,
{
    let mut wtr = csv::Writer::from_writer(writer);
    for record in records.iter() {
        wtr.serialize(record)?;
    }
    wtr.flush()?;
    Ok(())
}
