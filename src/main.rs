fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    taxc::cli::run()
}
