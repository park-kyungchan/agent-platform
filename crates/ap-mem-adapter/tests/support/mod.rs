pub mod upstream;

/// Explicit positive fixture profile for the pre-publisher harness tests.
/// Publisher acceptance must start with Server::start's hostile Behaviour default.
#[allow(dead_code)]
pub fn fixture_server(fixture: serde_json::Value) -> upstream::Server {
    let server = upstream::Server::start(fixture);
    assert!(server.address().ip().is_loopback());
    server.edit(|m| {
        m.behaviour.fault = None;
        m.behaviour.require_initialize = false;
        m.behaviour.root_routes = true;
        m.behaviour.provenance = true;
        m.behaviour.complete_reads = true;
        m.behaviour.config_resolved = true;
        m.behaviour.observable_operations = true;
        m.behaviour.exact_retained_body = true;
        m.behaviour.replace_tags = true;
        m.behaviour.replace_facts = true;
        m.behaviour.echo_created = true;
        m.behaviour.tree_bodies = true;
    });
    server
}
