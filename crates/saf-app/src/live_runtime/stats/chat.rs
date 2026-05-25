use super::super::support::strip_minecraft_color_codes;
use saf_core::numbers::parse_compact_number;

pub(crate) fn parse_hypixel_ping_ms(text: &str) -> Option<u64> {
    let cleaned = strip_minecraft_color_codes(text);
    let rest = cleaned.split_once("Your Ping - ")?.1;
    let raw = rest.split_once("ms")?.0.trim().replace(',', "");
    raw.parse::<u64>().ok()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PurchaseChatMessage {
    pub(crate) item_name: String,
    pub(crate) price: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SoldChatMessage {
    pub(crate) buyer: String,
    pub(crate) item_name: String,
    pub(crate) price: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClaimedSoldChatMessage {
    pub(crate) coins: u64,
    pub(crate) item_name: String,
    pub(crate) buyer: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OwnAuctionCollectionMessage {
    pub(crate) collector: String,
    pub(crate) coins: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ChatStatsUpdate {
    pub(crate) purchase: Option<PurchaseStatsUpdate>,
    pub(crate) sold: Option<SoldStatsUpdate>,
    pub(crate) claim: Option<ClaimStatsUpdate>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PurchaseStatsUpdate {
    pub(crate) auction_id: String,
    pub(crate) item_name: String,
    pub(crate) weird_item_name: String,
    pub(crate) tag: Option<String>,
    pub(crate) price: u64,
    pub(crate) target_price: f64,
    pub(crate) profit: f64,
    pub(crate) finder: String,
    pub(crate) volume: Option<f64>,
    pub(crate) profit_percentage: Option<f64>,
    pub(crate) buy_kind: String,
    pub(crate) buy_speed_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SoldStatsUpdate {
    pub(crate) buyer: String,
    pub(crate) item_name: String,
    pub(crate) price: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClaimStatsUpdate {
    pub(crate) coins: u64,
    pub(crate) item_name: String,
    pub(crate) buyer: String,
}

pub(crate) fn parse_purchase_chat_message(text: &str) -> Option<PurchaseChatMessage> {
    let cleaned = strip_minecraft_color_codes(text);
    let cleaned = cleaned.trim();
    let rest = cleaned
        .strip_prefix("You purchased ")
        .or_else(|| cleaned.strip_prefix("You bought "))?;
    let (item_name, price) = rest.rsplit_once(" for ")?;
    let price = price
        .trim()
        .trim_end_matches('!')
        .trim()
        .strip_suffix("coins")?
        .trim();
    Some(PurchaseChatMessage {
        item_name: item_name.trim().to_string(),
        price: parse_coin_u64(price)?,
    })
}

pub(crate) fn parse_sold_chat_message(text: &str) -> Option<SoldChatMessage> {
    let cleaned = strip_minecraft_color_codes(text);
    let cleaned = cleaned.trim();
    let rest = cleaned.strip_prefix("[Auction]")?.trim();
    let (buyer, sale) = rest.split_once(" bought ")?;
    let (item_name, price) = sale.rsplit_once(" for ")?;
    let price = price.split_once(" coins").map(|(price, _)| price)?;
    Some(SoldChatMessage {
        buyer: buyer.trim().to_string(),
        item_name: item_name.trim().to_string(),
        price: parse_coin_u64(price)?,
    })
}

pub(crate) fn parse_claimed_sold_chat_message(text: &str) -> Option<ClaimedSoldChatMessage> {
    let cleaned = strip_minecraft_color_codes(text);
    let cleaned = cleaned.trim();
    let rest = cleaned.strip_prefix("You collected ")?;
    let (coins, sale) = rest.split_once(" coins from selling ")?;
    let (item_name, buyer) = sale.rsplit_once(" to ")?;
    let buyer = buyer
        .trim()
        .trim_end_matches('!')
        .strip_suffix(" in an auction")
        .unwrap_or_else(|| buyer.trim().trim_end_matches('!'))
        .trim();
    if buyer.is_empty() {
        return None;
    }
    Some(ClaimedSoldChatMessage {
        coins: parse_coin_u64(coins)?,
        item_name: item_name.trim().to_string(),
        buyer: buyer.to_string(),
    })
}

pub(crate) fn parse_own_auction_collection_message(
    text: &str,
) -> Option<OwnAuctionCollectionMessage> {
    let cleaned = strip_minecraft_color_codes(text);
    let cleaned = cleaned.trim();
    let (collector, rest) = cleaned.split_once(" collected an auction for ")?;
    let coins = rest
        .trim()
        .trim_end_matches('!')
        .trim()
        .strip_suffix("coins")?
        .trim();
    Some(OwnAuctionCollectionMessage {
        collector: clean_auction_player_name(collector),
        coins: parse_coin_u64(coins)?,
    })
}

fn clean_auction_player_name(name: &str) -> String {
    strip_minecraft_color_codes(name)
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .to_string()
}

fn parse_coin_u64(text: &str) -> Option<u64> {
    let cleaned = text.trim().trim_end_matches('!').trim();
    let amount = cleaned
        .strip_suffix("coins")
        .or_else(|| cleaned.strip_suffix("coin"))
        .unwrap_or(cleaned)
        .trim();

    if let Some(value) = parse_compact_number(amount)
        && value.is_finite()
        && value >= 0.0
        && value <= u64::MAX as f64
    {
        return Some(value.round() as u64);
    }

    let digits = amount
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse::<u64>().ok())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_parser_preserves_compact_suffixes() {
        assert_eq!(parse_coin_u64("350.01k"), Some(350_010));
        assert_eq!(parse_coin_u64("350.01k coins"), Some(350_010));
        assert_eq!(parse_coin_u64("20M"), Some(20_000_000));
        assert_eq!(parse_coin_u64("1.5b"), Some(1_500_000_000));
        assert_eq!(parse_coin_u64("350,010"), Some(350_010));
    }

    #[test]
    fn claimed_sold_chat_handles_compact_collection_amounts() {
        let message = parse_claimed_sold_chat_message(
            "You collected 350.01k coins from selling Test Item to Buyer in an auction!",
        )
        .expect("compact collection should parse");

        assert_eq!(message.coins, 350_010);
        assert_eq!(message.item_name, "Test Item");
        assert_eq!(message.buyer, "Buyer");
    }
}
