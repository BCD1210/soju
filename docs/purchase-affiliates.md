# Purchase referrals

Soju remains free. The first integration targets GOG product listings in Discover.
Store search and result ordering do not depend on affiliate eligibility.
Checkout remains in the official store in the user's browser.

## Current status

- GOG: application emailed to affiliate@gog.com on 2026-09-06. Asked for
  desktop-app placement permission, the approved network/link format,
  commission and attribution terms, and catalog/search permission.
- Green Man Gaming: official Business Affiliate Program leads to Impact.
  Sign-in is required; no application has been submitted.
- No affiliate links are active and no commissions are being claimed.

## Activation

Only activate after written acceptance of Soju and its desktop-app placement.
Obtain the links from Soju's approved publisher account; never reuse another
project's affiliate identifiers. Store public links only, never credentials.

Edit resources/purchase-links.json with schema 1, enabled true, and links
containing exact destination and affiliate URL pairs. Destinations must match
the canonical GOG product URL returned by Discover. The initial implementation
accepts HTTPS www.gog.com and af.gog.com affiliate links only. If the approved
network uses another host or dynamic deep-link format, add a reviewed adapter
and destination-preservation tests before activation. Do not guess tracking
parameters or turn a search result into a different product or edition.

Before release, verify each approved link reaches the same product, region and
edition, and confirm attribution using the network's approved test procedure.
Do not make self-purchases to generate commissions. Record the approval and
private account details outside this public repository.

Discover labels affiliated product buttons and explains that Soju may earn a
commission. A persistent toggle lets users open ordinary store links instead.
No click collection, user identifiers, redirect server or background requests
are added by the referral layer. Third-party tracking starts only after a user
chooses an affiliated link. Invalid or missing configuration falls back to
ordinary store links. Disable the bundled configuration to stop referrals.

## Sources

- https://support.gog.com/hc/en-us/articles/4405004689297-How-to-join-the-GOG-Affiliate-Program
- https://affiliate.gog.com/
- https://www.greenmangaming.com/affiliates/
