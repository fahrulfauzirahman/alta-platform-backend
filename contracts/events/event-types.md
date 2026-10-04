# Event types

- `reference_item.created.v1`
  - payload: `{ id: uuid, title: string, created_at: datetime }`
  - tenant-scoped, ordered per tenant by `created_at`.
