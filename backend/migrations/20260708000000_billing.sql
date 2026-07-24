-- Stripe subscription billing (BILLING=on deployments only; columns are inert
-- otherwise). One subscription per user; state is mirrored from Stripe via
-- webhooks. subscription_status holds the Stripe status verbatim ('none' until
-- the user ever subscribes); current_period_end is the paid-through timestamp.

ALTER TABLE users
    ADD COLUMN stripe_customer_id text,
    ADD COLUMN subscription_status text NOT NULL DEFAULT 'none',
    ADD COLUMN current_period_end timestamp with time zone;

CREATE UNIQUE INDEX users_stripe_customer_id_key
    ON users (stripe_customer_id)
    WHERE stripe_customer_id IS NOT NULL;
