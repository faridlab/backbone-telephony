//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    pub(crate) write_service: Arc<TelephonyWriteService>,
    pub(crate) event_sink: Arc<dyn TelephonyEventSink>,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        // Validated write engine — the CDR dedup + idempotent insert + subject link + same-tx
        // outbox event (the outbox record stays company-keyed per ADR-0011). The in-process sink
        // defaults to LoggingSink; the durable path is the outbox, so this default is safe for
        // single-process composition.
        let write_service = Arc::new(TelephonyWriteService::new(db_pool.clone()));
        let event_sink: Arc<dyn TelephonyEventSink> = Arc::new(LoggingSink);
        Ok(ModuleExt {
            write_service,
            event_sink,
        })
    }
}

impl crate::ThisModule {
    /// The production surface: read-only base (`GET /calls…`) merged with the validated write
    /// path (`POST /calls` CDR ingest, `POST /calls/:id/link` subject attach). Generic mutation
    /// cannot reach here, so the CDR dedup, the outbox event, and the subject link all enforce;
    /// tenant isolation is whatever fence the composing service installs (ADR-0029). Prefer this
    /// over `all_crud_routes()`.
    pub fn validated_routes(&self) -> Router {
        use presentation::http::create_telephony_validated_write_routes;
        self.readonly_routes()
            .merge(create_telephony_validated_write_routes(
                self.write_service.clone(),
                self.event_sink.clone(),
            ))
    }
    /// Override the in-process event sink (default is `LoggingSink`). The durable path is the
    /// transactional outbox staged inside `record_call`; this sink is only a low-fanout notify,
    /// so the default is safe for single-process composition — supply a bus-backed sink for
    /// fan-out to multiple consumers.
    pub fn with_event_sink(mut self, sink: Arc<dyn TelephonyEventSink>) -> Self {
        self.event_sink = sink;
        self
    }
    /// The validated CDR write service (dedup + idempotent insert + subject link + outbox).
    pub fn write_service(&self) -> &Arc<TelephonyWriteService> {
        &self.write_service
    }
    /// The in-process event sink the validated write path publishes to.
    pub fn event_sink(&self) -> &Arc<dyn TelephonyEventSink> {
        &self.event_sink
    }
}
