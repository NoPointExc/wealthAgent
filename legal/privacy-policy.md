# Privacy Policy

**Texas Net Worth LLC — WealthAgent**

**Last updated: August 1, 2026**

> **Draft — not legal advice.** This document was prepared as a starting point and
> reflects how the WealthAgent service actually handles data. Before you publish it,
> have a licensed attorney review it against your final business practices and the
> jurisdictions you operate in. Items in **[brackets]** are placeholders you must fill in.

---

## 1. Who we are

Texas Net Worth LLC ("**Texas Net Worth**," "**we**," "**us**," or "**our**") operates
**WealthAgent**, a personal-finance aggregation service that connects your bank, brokerage,
credit-card, and loan accounts into a single dashboard and exposes them, read-only, to the
AI agents you choose (the "**Service**"). The Service is available at
`texasnetworth.com`, `app.texasnetworth.com`, and related subdomains.

This Privacy Policy explains what information we collect, how we use it, who we share it
with, and the choices you have. It applies to the Service and does not apply to any
third-party product you connect to it (for example, your bank, or an AI provider such as
Anthropic, OpenAI, or Google).

Contact: **support@texasnetworth.com**
Mailing address: **Texas Net Worth LLC, 5900 Balcones Drive STE 100, Austin, TX 78731, USA**

## 2. A short summary

- We collect the financial account data you ask us to aggregate, the profile information
  from your sign-in, and the technical data needed to run and secure the Service.
- **We do not sell your personal information, and we never use your financial data to
  train machine-learning or AI models.**
- We use **Plaid** to connect to your financial institutions. You give Plaid your bank
  credentials — **we never see or store them.**
- Payments are handled by **Stripe**. We never see or store your full card number.
- Your data is shared with an AI agent **only when you connect one and authorize it.**
- You can disconnect an institution, delete your data, or close your account at any time.

## 3. Information we collect

### 3.1 Information you provide

- **Account / identity information.** When you sign up with Google Sign-In, we receive your
  name, email address, and a Google account identifier. We use this to create and secure
  your account.
- **Content you create in the app.** Transaction tags and notes, account nicknames, cost-basis
  entries, saved searches, and similar annotations you add to your data.
- **Support communications.** If you email us, we keep your messages and contact details.

### 3.2 Financial data (via Plaid)

To aggregate your accounts we use **Plaid Inc.** ("Plaid"). You enter your financial-institution
credentials directly with Plaid; **Texas Net Worth never receives or stores your banking login
credentials.** Through Plaid, and only for the accounts you connect, we receive and store:

- Account metadata: institution name, account name, account type/subtype, and a masked account
  number;
- Balances;
- Transactions: amount, date, description, category, and merchant;
- Investment holdings and securities information;
- Investment (securities) transactions.

Your use of Plaid is also governed by
**Plaid's End User Privacy Policy** (https://plaid.com/legal/#end-user-privacy-policy).

### 3.3 Billing information (via Stripe)

If you subscribe, payments are processed by **Stripe, Inc.** ("Stripe"). Stripe collects and
stores your payment-card and billing details. We receive from Stripe only a customer identifier,
your subscription status, and your billing-period dates — **we do not receive or store your full
card number.** Stripe's handling of your data is governed by Stripe's Privacy Policy
(https://stripe.com/privacy).

### 3.4 Information collected automatically

- **Log and device data:** IP address, browser/device type, pages and actions, and timestamps.
- **Cookies:** We use a single, essential session cookie to keep you signed in. We do not use
  advertising cookies. See Section 8.
- **Security data:** rate-limiting and abuse-prevention signals.

### 3.5 The optional Privacy Lock

The Service offers an optional passphrase-based "Privacy Lock" that encrypts your financial
fields at rest with a key derived from a passphrase you choose. **We do not store your
passphrase in plaintext and cannot recover it** — if you lose it, encrypted data cannot be
decrypted. This feature is your choice and off by default.

## 4. How we use information

We use the information above to:

- Provide, operate, aggregate, and continuously sync the Service;
- Calculate net worth, capital gains, spending, and similar views you request;
- Authenticate you and keep your account and data secure;
- Process subscriptions, billing, and taxes;
- Respond to support requests and send you service-related messages (for example, sync or
  payment notices);
- Detect, prevent, and address fraud, abuse, and security incidents;
- Comply with legal obligations and enforce our Terms of Service.

**We never sell your personal information, and we never use your financial data to train,
fine-tune, or improve machine-learning or AI models.**

## 5. How we share information

We share information only as described here:

- **Service providers (subprocessors)** who process data on our behalf, listed in Section 6.
- **AI agents you connect.** When you connect a third-party AI client over MCP (for example
  Claude, ChatGPT, or Gemini) using a personal access token or an OAuth authorization you
  grant, that agent can read the financial data you authorize. The AI provider is a third
  party you choose; its handling of your data is governed by **its** terms and privacy policy,
  not ours. You control what you connect and can revoke access at any time.
- **Legal and safety.** When required by law, subpoena, or legal process, or to protect the
  rights, property, or safety of Texas Net Worth, our users, or the public.
- **Business transfers.** In connection with a merger, acquisition, financing, or sale of
  assets, subject to this Policy.
- **With your direction or consent** for any other disclosure.

## 6. Subprocessors and third parties

We rely on the following third parties to run the Service:

| Provider | Purpose | More information |
|---|---|---|
| Plaid Inc. | Financial-account aggregation | https://plaid.com/legal/ |
| Stripe, Inc. | Payment and subscription processing | https://stripe.com/privacy |
| Google LLC | Sign-in / authentication | https://policies.google.com/privacy |
| DigitalOcean, LLC | Cloud hosting and infrastructure (United States) | https://www.digitalocean.com/legal/privacy-policy |

AI providers you connect over MCP are **not** our subprocessors — they act under your direction.

## 7. Data retention and deletion

We keep your information for as long as your account is active or as needed to provide the
Service. You can:

- **Disconnect an institution** at any time, which stops syncing that connection and instructs
  Plaid to remove the connection;
- **Reset your data** from within the app;
- **Close your account**, after which we delete your personal and financial data from our active
  systems and instruct Plaid to remove your connections.

Residual copies may persist in encrypted backups for a limited period and are then deleted on
our normal backup-rotation schedule. We may retain limited records where required for legal,
tax, or fraud-prevention purposes.

To close your account or request deletion, use the in-app controls or email
**support@texasnetworth.com**.

## 8. Cookies

We use one essential, first-party session cookie to keep you logged in. It is required for the
Service to function and is not used for advertising or cross-site tracking. Blocking it will
prevent you from signing in.

## 9. Security

We protect your data with:

- Encryption in transit (HTTPS/TLS) for all traffic;
- Encryption at rest for sensitive fields, including your Plaid access tokens;
- The optional passphrase-based Privacy Lock (Section 3.5);
- Scoped, revocable personal access tokens for AI-agent connections;
- Access controls, rate limiting, and monitoring.

No method of transmission or storage is 100% secure, and we cannot guarantee absolute security.

## 10. Your rights and choices

Depending on where you live, you may have the right to access, correct, delete, or export your
personal information, to object to or restrict certain processing, and to withdraw consent.
Because **we do not sell personal information or share it for cross-context behavioral
advertising**, there is nothing to opt out of in that respect.

- **California residents (CCPA/CPRA):** you have the rights described above and the right not to
  be discriminated against for exercising them.
- **EEA/UK residents (GDPR):** our legal bases are performance of our contract with you, your
  consent, our legitimate interests in operating and securing the Service, and compliance with
  law. You may lodge a complaint with your supervisory authority.

To exercise any right, email **support@texasnetworth.com**. We will verify your request and
respond within the time required by applicable law.

## 11. Children's privacy

The Service is not directed to, and is not intended for, anyone under 18. We do not knowingly
collect personal information from children. If you believe a child has provided us information,
contact us and we will delete it.

## 12. International users

We operate the Service from the United States, and your information is processed and stored in
the United States. If you access the Service from outside the U.S., you consent to that transfer
and processing.

## 13. Changes to this Policy

We may update this Policy from time to time. If we make material changes, we will notify you by
email or through the Service and update the "Last updated" date above. Your continued use of the
Service after the changes take effect means you accept the revised Policy.

## 14. Contact us

Questions about this Policy or your data:

**Texas Net Worth LLC**
Email: **support@texasnetworth.com**
Address: **5900 Balcones Drive STE 100, Austin, TX 78731, USA**
