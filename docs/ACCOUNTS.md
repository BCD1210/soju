# Accounts and Discover

Soju 1.6 adds owned-library imports and store discovery to the native app.

| Feature | Steam | GOG | Epic Games | Battle.net |
| --- | --- | --- | --- | --- |
| Installed games in Soju environments | Yes | Yes | Yes | Yes |
| Owned, uninstalled games | Official Steam sign-in | Galaxy local library import | Not yet | Not yet |
| Search results inside Soju | Yes | Yes | Official store link | Official store link |
| Purchases and downloads | Official client/store | Official client/store | Official client/store | Official client/store |

## Connect Steam

1. Open **Accounts → Sign in to Steam** in Soju.
2. Complete sign-in on the official Steam page, including QR sign-in or Steam Guard if requested.
3. Soju imports the signed-in account's game list when it becomes available.
4. Later, **Sync Steam library** opens the same flow and reuses the web session while Steam allows it.
   If the page cannot finish loading, choose **Retry import**. Cancel keeps the previous import.

No personal API key or profile URL is required. The sign-in window shows the current official Steam host.
Your password and Steam Guard entries go to Steam's own page; Soju does not provide a password form or read its fields.
WebKit keeps the web session in Soju's local website data, separate from your browser and Steam client.
Disconnect removes this local session. It does not log out other browsers or the official launcher.

The importer reads the signed-in user's All Games page or calls Valve's
[GetOwnedGames service](https://partner.steamgames.com/doc/webapi/iplayerservice#GetOwnedGames)
within Steam's web page using that page's existing session. Only the Steam account ID, game IDs and names
are passed to the local library helper; the saved account label is masked. Session cookies and web tokens are never sent to the helper,
written into the library snapshot or logged by Soju.

This is an embedded official website sign-in, not a Valve-approved OAuth integration.
Steam's website data format can change. An unavailable, private, incomplete or failed response leaves the
last imported library unchanged. Free games the account has played are included when Steam returns them.
Purchases, installations and the Windows Steam client's login still belong to the official client/store.

## Import GOG GALAXY

1. Open **GOG GALAXY**, sign in, and let the owned game library finish loading.
2. Return to **Accounts** in Soju and choose **Refresh account list**.
3. If Galaxy has cached multiple accounts, choose the account to import.
4. Choose **Import from Galaxy**.

The importer reads Galaxy's local database in read-only mode. It checks the selected account's ownership records,
excludes DLC records, and obtains game titles from local product/title metadata.
It does not read passwords, authentication tokens, activation keys or product authorization secrets.
Galaxy itself contacts GOG to refresh its account library; Soju imports the snapshot already saved on this Mac.
If the database is busy, incomplete or has an unsupported schema, the previous import remains intact.

## Use your library

**All games**, **Installed** and **Not installed** filters distinguish local installations from imported ownership.
When a game is both installed and imported, the current installation record wins, so it appears once.
An imported entry alone is never treated as proof that a game is installed or runnable.

Steam's **Install** button opens the official Steam installation flow.
GOG's **Galaxy** button opens Galaxy; choose the game there to install it.
Set up the corresponding platform first if it is not installed. Return to Soju and refresh after installation.
The ordinary Play action always rechecks the local installation record.

Account snapshots are saved under `~/.battlenet-macos/account-libraries` with private file permissions.
Favorites and recently opened game requests remain local. **Disconnect** removes the selected import and, for Steam,
its saved Soju web session. Installed games are preserved. Imports are manual snapshots; they do not update automatically in the background.

## Discover

Enter a game name and choose the store region: US/USD, Korea/KRW, UK/GBP, Germany/EUR or Japan/JPY.
Soju searches the public Steam and GOG catalog endpoints in parallel.
Results include titles, store artwork, prices where returned, and an installed/owned badge for matching platform IDs.
A failed store does not hide results from the other store. Missing prices are labeled **View price in store**.

**View in store** opens the official product page in your default browser. Search links are also available for
Steam, GOG, Epic Games and Battle.net. No purchases are submitted by Soju.

Listings may include DLC and bundles. Prices are region-dependent and can change at checkout.
Store availability or a listed macOS version is not a compatibility guarantee for the Windows version running through Soju.
Search queries go to Steam/GOG and result artwork loads from their CDNs. Your library is not uploaded to either store
to produce ownership badges, and no telemetry or analytics are added.

## CLI and maintenance

```bash
soju library
soju game <installed-id>
soju game-install <owned-id>
soju search "The Witcher 3" --country US
```

The Steam library reader executes inside the official Steam page and returns a metadata-only snapshot.
The local service bridge accepts that snapshot through stdin and validates it before replacing the saved import.
No Steam password, cookie, token or API key is accepted by the helper.
`library_services.py` owns provider parsing, bounded responses, read-only Galaxy queries and atomic local imports.

Steam's website, store catalogs and Galaxy's local schema can change independently of Soju.
Provider failures are shown as recoverable errors rather than empty successful account imports.
Tests cover account separation, ownership/DLC filtering, private Steam responses, cache preservation, secret-free errors,
private cache permissions, safe product/artwork URLs, partial search failures and installed/owned merging.
