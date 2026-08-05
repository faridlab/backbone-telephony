-- Rollback the telephony outbox tables. The relay must be drained first in production; the
-- inbox_consumed + outbox_events tables are module-local and safe to drop here.
DROP TABLE IF EXISTS telephony.inbox_consumed;
DROP TABLE IF EXISTS telephony.outbox_events;
