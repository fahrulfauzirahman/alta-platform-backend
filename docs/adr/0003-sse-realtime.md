# ADR 0003: SSE realtime

`GET /v1/events` streams tenant-filtered `reference_item.created.v1` with heartbeat + reconnect. No Kafka/NATS in foundation.
