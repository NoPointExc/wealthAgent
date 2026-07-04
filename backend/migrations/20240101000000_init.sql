-- WealthAgent schema — consolidated from the original incremental migrations,
-- including the optional operator-blind privacy encryption columns
-- (PRIVACY_ENCRYPTION=on; see backend/src/privacy.rs for the key hierarchy).
--
-- Privacy columns follow one convention: for encrypted users the plaintext
-- column holds '' / NULL and <field>_enc holds an X25519 sealed box.
-- Sentinel symbols (CASH/DEBT/N/A) and synthetic cash/debt holdings stay
-- plaintext — they reveal nothing. security_id stays plaintext (holdings
-- upsert key); residual risk: Plaid security_ids are resolvable by an
-- operator with their own Plaid credentials (see README threat model).


-- Name: account_snapshots; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.account_snapshots (
    account_id text NOT NULL,
    snapshot_date date NOT NULL,
    balance_cents bigint NOT NULL
);

-- Name: accounts; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.accounts (
    id text NOT NULL,
    name text NOT NULL,
    balance bigint NOT NULL,
    trend_pct double precision NOT NULL,
    plaid_item_id text,
    custom_name text,
    account_type text DEFAULT 'asset'::text NOT NULL,
    name_enc bytea,
    custom_name_enc bytea,
    CONSTRAINT accounts_account_type_check CHECK ((account_type = ANY (ARRAY['asset'::text, 'liability'::text])))
);

-- Name: cost_basis_overrides; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.cost_basis_overrides (
    id bigint NOT NULL,
    user_id text NOT NULL,
    investment_transaction_id text NOT NULL,
    cost_basis_cents bigint NOT NULL,
    is_long_term boolean DEFAULT false NOT NULL,
    source text DEFAULT 'user_input'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);

-- Name: cost_basis_overrides_id_seq; Type: SEQUENCE; Schema: public; Owner: -

CREATE SEQUENCE public.cost_basis_overrides_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- Name: cost_basis_overrides_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -

ALTER SEQUENCE public.cost_basis_overrides_id_seq OWNED BY public.cost_basis_overrides.id;

-- Name: holdings; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.holdings (
    id bigint NOT NULL,
    symbol text NOT NULL,
    account_id text NOT NULL,
    name text NOT NULL,
    asset_value bigint NOT NULL,
    return_str text NOT NULL,
    performance_type text NOT NULL,
    security_id text,
    quantity double precision,
    symbol_enc bytea,
    name_enc bytea
);

-- Name: holdings_id_seq; Type: SEQUENCE; Schema: public; Owner: -

CREATE SEQUENCE public.holdings_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- Name: holdings_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -

ALTER SEQUENCE public.holdings_id_seq OWNED BY public.holdings.id;

-- Name: investment_transactions; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.investment_transactions (
    id text NOT NULL,
    account_id text NOT NULL,
    date date NOT NULL,
    name text NOT NULL,
    amount bigint NOT NULL,
    quantity double precision,
    price bigint,
    fees bigint,
    txn_type text NOT NULL,
    subtype text,
    symbol text,
    security_name text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    name_enc bytea,
    symbol_enc bytea,
    security_name_enc bytea
);

-- Name: invited_emails; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.invited_emails (
    email text NOT NULL,
    note text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- Name: oauth_auth_codes; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.oauth_auth_codes (
    id text NOT NULL,
    code_prefix text NOT NULL,
    code_hash text NOT NULL,
    client_id text NOT NULL,
    user_id text NOT NULL,
    redirect_uri text NOT NULL,
    scope text NOT NULL,
    resource text,
    code_challenge text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone
);

-- Name: oauth_clients; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.oauth_clients (
    client_id text NOT NULL,
    client_name text NOT NULL,
    redirect_uris text[] NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- Name: oauth_refresh_tokens; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.oauth_refresh_tokens (
    id text NOT NULL,
    token_prefix text NOT NULL,
    token_hash text NOT NULL,
    user_id text NOT NULL,
    client_id text NOT NULL,
    scope text NOT NULL,
    resource text,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    rotated_to text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone
);

-- Name: personal_api_tokens; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.personal_api_tokens (
    id text NOT NULL,
    user_id text NOT NULL,
    name text NOT NULL,
    token_prefix text NOT NULL,
    token_hash text NOT NULL,
    scopes text DEFAULT 'read,write,sync'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    revoked_at timestamp with time zone,
    -- Privacy: a copy of the user's private key wrapped under a KEK derived
    -- from the raw token value; opaque without the token (DB stores only a hash).
    wrapped_private_key bytea
);

-- Name: plaid_items; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.plaid_items (
    id text NOT NULL,
    institution_id text,
    created_at timestamp with time zone DEFAULT now(),
    user_id text,
    access_token_enc bytea NOT NULL,
    key_version integer DEFAULT 1 NOT NULL,
    revoked_at timestamp with time zone
);

-- Name: saved_searches; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.saved_searches (
    id integer NOT NULL,
    name text NOT NULL,
    search_query text,
    selected_account text,
    selected_categories text,
    start_date text,
    end_date text,
    amount_mode text,
    amount_val text,
    amount_min text,
    amount_max text,
    show_pending_only boolean DEFAULT false NOT NULL,
    user_id text,
    selected_tags text
);

-- Name: saved_searches_id_seq; Type: SEQUENCE; Schema: public; Owner: -

CREATE SEQUENCE public.saved_searches_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;

-- Name: saved_searches_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -

ALTER SEQUENCE public.saved_searches_id_seq OWNED BY public.saved_searches.id;

-- Name: transactions; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.transactions (
    id text NOT NULL,
    account_id text NOT NULL,
    txn_date text NOT NULL,
    raw_string text NOT NULL,
    merchant_name text,
    amount bigint NOT NULL,
    pending boolean DEFAULT false NOT NULL,
    payment_channel text,
    plaid_category text,
    plaid_primary_category text,
    plaid_detailed_category text,
    tags text,
    note text,
    raw_string_enc bytea,
    merchant_name_enc bytea,
    note_enc bytea
);

-- Name: users; Type: TABLE; Schema: public; Owner: -

CREATE TABLE public.users (
    id text NOT NULL,
    google_id text NOT NULL,
    email text NOT NULL,
    name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

-- Name: cost_basis_overrides id; Type: DEFAULT; Schema: public; Owner: -

ALTER TABLE ONLY public.cost_basis_overrides ALTER COLUMN id SET DEFAULT nextval('public.cost_basis_overrides_id_seq'::regclass);

-- Name: holdings id; Type: DEFAULT; Schema: public; Owner: -

ALTER TABLE ONLY public.holdings ALTER COLUMN id SET DEFAULT nextval('public.holdings_id_seq'::regclass);

-- Name: saved_searches id; Type: DEFAULT; Schema: public; Owner: -

ALTER TABLE ONLY public.saved_searches ALTER COLUMN id SET DEFAULT nextval('public.saved_searches_id_seq'::regclass);

-- Name: account_snapshots account_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.account_snapshots
    ADD CONSTRAINT account_snapshots_pkey PRIMARY KEY (account_id, snapshot_date);

-- Name: accounts accounts_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.accounts
    ADD CONSTRAINT accounts_pkey PRIMARY KEY (id);

-- Name: cost_basis_overrides cost_basis_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.cost_basis_overrides
    ADD CONSTRAINT cost_basis_overrides_pkey PRIMARY KEY (id);

-- Name: cost_basis_overrides cost_basis_overrides_user_id_investment_transaction_id_key; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.cost_basis_overrides
    ADD CONSTRAINT cost_basis_overrides_user_id_investment_transaction_id_key UNIQUE (user_id, investment_transaction_id);

-- Name: holdings holdings_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.holdings
    ADD CONSTRAINT holdings_pkey PRIMARY KEY (id);

-- Name: investment_transactions investment_transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.investment_transactions
    ADD CONSTRAINT investment_transactions_pkey PRIMARY KEY (id);

-- Name: invited_emails invited_emails_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.invited_emails
    ADD CONSTRAINT invited_emails_pkey PRIMARY KEY (email);

-- Name: oauth_auth_codes oauth_auth_codes_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_auth_codes
    ADD CONSTRAINT oauth_auth_codes_pkey PRIMARY KEY (id);

-- Name: oauth_clients oauth_clients_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_clients
    ADD CONSTRAINT oauth_clients_pkey PRIMARY KEY (client_id);

-- Name: oauth_refresh_tokens oauth_refresh_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_pkey PRIMARY KEY (id);

-- Name: personal_api_tokens personal_api_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.personal_api_tokens
    ADD CONSTRAINT personal_api_tokens_pkey PRIMARY KEY (id);

-- Name: plaid_items plaid_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.plaid_items
    ADD CONSTRAINT plaid_items_pkey PRIMARY KEY (id);

-- Name: saved_searches saved_searches_name_key; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.saved_searches
    ADD CONSTRAINT saved_searches_name_key UNIQUE (name);

-- Name: saved_searches saved_searches_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.saved_searches
    ADD CONSTRAINT saved_searches_pkey PRIMARY KEY (id);

-- Name: transactions transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_pkey PRIMARY KEY (id);

-- Name: users users_google_id_key; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_google_id_key UNIQUE (google_id);

-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

-- Name: idx_cbo_user; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_cbo_user ON public.cost_basis_overrides USING btree (user_id);

-- Name: idx_holdings_sec_acc; Type: INDEX; Schema: public; Owner: -

CREATE UNIQUE INDEX idx_holdings_sec_acc ON public.holdings USING btree (security_id, account_id);

-- Name: idx_inv_txn_account_date; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_inv_txn_account_date ON public.investment_transactions USING btree (account_id, date DESC);

-- Name: idx_oauth_code_prefix; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_oauth_code_prefix ON public.oauth_auth_codes USING btree (code_prefix) WHERE (consumed_at IS NULL);

-- Name: idx_oauth_refresh_prefix; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_oauth_refresh_prefix ON public.oauth_refresh_tokens USING btree (token_prefix) WHERE (revoked_at IS NULL);

-- Name: idx_oauth_refresh_user; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_oauth_refresh_user ON public.oauth_refresh_tokens USING btree (user_id, client_id);

-- Name: idx_pat_prefix; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_pat_prefix ON public.personal_api_tokens USING btree (token_prefix) WHERE (revoked_at IS NULL);

-- Name: idx_pat_user; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_pat_user ON public.personal_api_tokens USING btree (user_id) WHERE (revoked_at IS NULL);

-- Name: idx_txn_account; Type: INDEX; Schema: public; Owner: -

CREATE INDEX idx_txn_account ON public.transactions USING btree (account_id);

-- Name: account_snapshots account_snapshots_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.account_snapshots
    ADD CONSTRAINT account_snapshots_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.accounts(id) ON DELETE CASCADE;

-- Name: cost_basis_overrides cost_basis_overrides_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.cost_basis_overrides
    ADD CONSTRAINT cost_basis_overrides_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- Name: holdings holdings_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.holdings
    ADD CONSTRAINT holdings_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.accounts(id);

-- Name: investment_transactions investment_transactions_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.investment_transactions
    ADD CONSTRAINT investment_transactions_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.accounts(id) ON DELETE CASCADE;

-- Name: oauth_auth_codes oauth_auth_codes_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_auth_codes
    ADD CONSTRAINT oauth_auth_codes_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;

-- Name: oauth_auth_codes oauth_auth_codes_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_auth_codes
    ADD CONSTRAINT oauth_auth_codes_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- Name: oauth_refresh_tokens oauth_refresh_tokens_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;

-- Name: oauth_refresh_tokens oauth_refresh_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- Name: personal_api_tokens personal_api_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.personal_api_tokens
    ADD CONSTRAINT personal_api_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;

-- Name: plaid_items plaid_items_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.plaid_items
    ADD CONSTRAINT plaid_items_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);

-- Name: saved_searches saved_searches_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.saved_searches
    ADD CONSTRAINT saved_searches_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id);

-- Name: transactions transactions_account_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -

ALTER TABLE ONLY public.transactions
    ADD CONSTRAINT transactions_account_id_fkey FOREIGN KEY (account_id) REFERENCES public.accounts(id);

-- PostgreSQL database dump complete


-- Name: user_privacy_keys; Type: TABLE; Schema: public; Owner: -
-- Optional operator-blind privacy encryption (PRIVACY_ENCRYPTION=on).
-- See backend/src/privacy.rs for the key hierarchy and sealed-box format.

CREATE TABLE public.user_privacy_keys (
    user_id text NOT NULL,
    -- X25519 public key, plaintext — the sync path seals new rows with it.
    public_key bytea NOT NULL,
    -- Private key wrapped under the passphrase-derived KEK (Argon2id).
    wrapped_private_key bytea NOT NULL,
    kdf_salt bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);

ALTER TABLE ONLY public.user_privacy_keys
    ADD CONSTRAINT user_privacy_keys_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY public.user_privacy_keys
    ADD CONSTRAINT user_privacy_keys_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;
