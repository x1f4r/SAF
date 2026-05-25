# Blacklist Rules

The bot has two separate blacklist scopes:

- `doNotBuy`: blocks buying before the final purchase click.
- `doNotRelist`: blocks listing/relisting after an item is claimed, recovered, or found in an expired auction flow.

Rules live in `config.json5` and are reloaded while the bot is running.

## Live Commands

List current rules:

```bash
cd /opt/saf
./saf.sh blacklist list
```

Add or remove a buy tag:

```bash
./saf.sh blacklist add buy tag LAVA_SHELL_NECKLACE
./saf.sh blacklist remove buy tag LAVA_SHELL_NECKLACE
```

Add `--for` to make a rule expire automatically:

```bash
./saf.sh blacklist add buy tag SPEED_RELIC --for 7d
```

Timed rules are active immediately and are ignored after their `expiresAt`
timestamp. The Discord `/blacklist` command has the same optional `duration`
field, for example `duration:7d`.

Add or remove a displayed item name:

```bash
./saf.sh blacklist add buy name 'Lava Shell Necklace'
./saf.sh blacklist remove buy name 'Lava Shell Necklace'
```

Add or remove a global enchantment block:

```bash
./saf.sh blacklist add buy enchant THE_ONE:5
./saf.sh blacklist remove buy enchant THE_ONE:5
```

Add or remove an item + enchantment combination:

```bash
./saf.sh blacklist add buy item-enchant LAVA_SHELL_NECKLACE THE_ONE:5
./saf.sh blacklist remove buy item-enchant LAVA_SHELL_NECKLACE THE_ONE:5
```

Use `relist` instead of `buy` for listing rules:

```bash
./saf.sh blacklist add relist item-enchant LAVA_SHELL_NECKLACE THE_ONE:5
```

## Current Lava Shell Rule

The Lava Shell Necklace problem is caused by the ultimate enchantment `The One V`, not by every Lava Shell Necklace. The safer rule is:

```json5
doNotBuy: {
  tags: [],
  names: [],
  enchantments: [],
  itemEnchantments: [
    { tag: "LAVA_SHELL_NECKLACE", enchantment: "THE_ONE", level: 5 }
  ]
},
doNotRelist: {
  tags: ["GIANTS_SWORD"],
  names: [],
  enchantments: [],
  itemEnchantments: [
    { tag: "LAVA_SHELL_NECKLACE", enchantment: "THE_ONE", level: 5 }
  ]
}
```

This allows normal Lava Shell Necklaces, but blocks Lava Shell Necklace items with `The One V` from both buying and relisting.

## How Enchantment Matching Works

The bot reads enchantments from:

- SkyBlock item NBT/custom data, for example `enchantments.ultimate_the_one = 5`.
- Item lore lines, for example `The One V`.

Enchantment names are normalized, so these refer to the same rule:

```text
THE_ONE:5
The One V
ultimate_the_one:5
```

Level-specific rules match that level or higher. A `THE_ONE:5` rule blocks level 5 and would not block level 4.

## Buy Latency Note

Tag and name rules can be checked from the SkyCofl flip payload before opening the auction. Enchantment rules usually require the actual Minecraft auction GUI item, so they are checked immediately after the auction window opens and before the bot clicks buy. This avoids buying the blocked item without adding extra pre-open latency to normal flips.
