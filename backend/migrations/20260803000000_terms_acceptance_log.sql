-- Append-only audit log of Terms of Service / Privacy Policy acceptances.
-- One row per time a user affirmatively clicks "I agree", capturing when, from
-- which IP/device, and which version — usable as proof of agreement.
-- users.terms_accepted_at / terms_version remain the fast "current state" columns;
-- this table is the immutable history.
CREATE TABLE terms_acceptances (
    id           text PRIMARY KEY,
    user_id      text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    terms_version text NOT NULL,
    accepted_at  timestamptz NOT NULL DEFAULT now(),
    ip_address   text,
    user_agent   text
);

CREATE INDEX idx_terms_acceptances_user ON terms_acceptances(user_id);
