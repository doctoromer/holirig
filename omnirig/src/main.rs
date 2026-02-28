fn main() -> Result<(), Box<dyn std::error::Error>> {
    omnirig::run_omnirig_server(omnirig::DummyProvider)
}
