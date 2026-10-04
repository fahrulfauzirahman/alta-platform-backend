// Verifies contracts/openapi/alta-platform-v1.yaml contains required paths.
#[test]
fn openapi_has_required_paths() {
    let yaml = include_str!("../../contracts/openapi/alta-platform-v1.yaml");
    for p in ["/healthz", "/readyz", "/v1/session", "/v1/reference-items", "/v1/events"] {
        assert!(yaml.contains(p), "missing {p}");
    }
}
