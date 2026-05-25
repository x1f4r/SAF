# Auction Flow

## Buy Path

1. SkyCofl sends a flip payload.
2. The bot validates the payload and checks fast buy blacklists using the item tag/name.
3. If the bot is idle and ready, it immediately sends `/viewauction <auction id>`.
4. When the `BIN Auction View` opens, the bot checks the actual auction item.
5. Enchantment blacklist rules are evaluated before the buy/confirm click.
6. If the auction is still valid, the bot clicks the action slot and handles nugget/bed purchase flows.

The sensitive latency path is the time between the SkyCofl flip event and `/viewauction`. Expensive flips use more aggressive timing settings. Extra safety checks should be placed after the auction window opens whenever possible.

## Claim And List Path

After a confirmed purchase:

1. The bot tracks the purchased auction id, expected item, price, target, and finder.
2. It claims the item from the auction/bid flow if Hypixel does not put it directly into inventory.
3. It finds the unique inventory UUID for the claimed item.
4. It checks relist rules, including tag/name/enchantment rules.
5. It opens the Auction House, creates a BIN auction, sets price and duration, verifies the GUI, and submits.
6. It records the listed auction so sold and expired handling can reconcile later.

If the auction house is full, bought items can be queued from inventory and listed later when a slot opens.

## Sold And Expired Reconciliation

The bot regularly reconciles auctions and also does this on startup/login.

Reconciliation checks:

- Sold auctions and coins to collect.
- Expired auctions/items to reclaim.
- Bought bid-section items that did not automatically move into inventory.
- Pending create-auction drafts that may contain an item stuck in the listing GUI.

When a sold item is collected, the bot should report the real collected coin amount instead of a stale zero-coin value.

## Relisting Expired Auctions

Expired auctions are reclaimed and can be relisted automatically when `doNotRelist.expiredAuctions` is enabled.

Before relisting an expired item, the bot checks:

- tag blacklist
- name blacklist
- enchantment blacklist
- item + enchantment blacklist
- skin settings
- profit/finder rules where applicable

## Private Island Behavior

The idle target location is the private island. The bot checks locraw/scoreboard state and avoids unnecessary `/is` warps when it is already on the island.

Stationary movement compatibility suppresses unnecessary movement packets after SkyBlock loads. This is intended to reduce suspicious or janky movement, especially after the AFK-pool issue.

