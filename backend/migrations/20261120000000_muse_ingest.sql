-- Push-based ingest from an external agent (Muse) that pulls from its own
-- Plaid connection and pushes the results over MCP.

-- Accounts are owned through plaid_items.user_id. Accounts first seen via a
-- push hang off a per-user 'muse' item that has no Plaid access token.
ALTER TABLE public.plaid_items ALTER COLUMN access_token_enc DROP NOT NULL;
ALTER TABLE public.plaid_items ADD COLUMN source text DEFAULT 'plaid' NOT NULL;
ALTER TABLE public.plaid_items
    ADD CONSTRAINT plaid_items_source_check CHECK (source = ANY (ARRAY['plaid'::text, 'muse'::text]));

-- Muse's Plaid connection issues its own account / transaction ids. This maps
-- each pushed (external) id onto the row it updates here, so history from the
-- original connection is reused instead of duplicated. Every pushed id gets a
-- row, including ids that created a fresh row (internal_id = external_id), so
-- "claimed" internal rows can never be fuzzy-matched a second time.
CREATE TABLE public.external_id_map (
    user_id text NOT NULL,
    kind text NOT NULL,
    external_id text NOT NULL,
    internal_id text NOT NULL,
    match_method text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT external_id_map_pkey PRIMARY KEY (user_id, kind, external_id),
    CONSTRAINT external_id_map_kind_check
        CHECK (kind = ANY (ARRAY['account'::text, 'transaction'::text, 'investment_transaction'::text]))
);

CREATE UNIQUE INDEX external_id_map_internal_key
    ON public.external_id_map USING btree (user_id, kind, internal_id);

CREATE TABLE public.ingest_log (
    id bigserial PRIMARY KEY,
    user_id text NOT NULL,
    tool text NOT NULL,
    dry_run boolean NOT NULL,
    ok boolean NOT NULL,
    summary jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

CREATE INDEX idx_ingest_log_user ON public.ingest_log USING btree (user_id, created_at DESC);
