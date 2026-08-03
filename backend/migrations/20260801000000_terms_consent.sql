-- Records a user's acceptance of the Terms of Service / Privacy Policy at signup.
-- terms_accepted_at is the timestamp of the most recent acceptance; terms_version
-- is the version string that was accepted (see TERMS_VERSION in handlers/auth.rs).
ALTER TABLE users
    ADD COLUMN terms_accepted_at timestamptz,
    ADD COLUMN terms_version text;
