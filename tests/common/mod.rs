pub fn test_tenant() -> alta_kernel::TenantId {
    alta_kernel::TenantId(uuid::Uuid::new_v4())
}
