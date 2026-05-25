use super::*;

impl BankRequest {
    pub(super) fn from_message(account: &str, message: &str) -> Result<Self, RuntimeError> {
        let args = message.split_whitespace().collect::<Vec<_>>();
        let withdraw = args.iter().any(|arg| arg.eq_ignore_ascii_case("withdraw"));
        let personal = args
            .iter()
            .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "personal" | "private"));
        let amount = args
            .iter()
            .find(|arg| {
                !matches!(
                    arg.to_ascii_lowercase().as_str(),
                    "withdraw" | "deposit" | "personal" | "private" | "shared" | "coop" | "co-op"
                )
            })
            .map(|value| (*value).to_string());

        let request = Self {
            amount,
            withdraw,
            personal,
        };
        required_bank_amount(account, &request)?;
        Ok(request)
    }
}

pub(super) fn build_ask_prefixes(configured: &[String]) -> Vec<(String, String)> {
    configured
        .iter()
        .map(|ign| {
            let lower = ign.to_ascii_lowercase();
            let prefix = (3..=10)
                .find_map(|len| {
                    let prefix = lower.chars().take(len).collect::<String>();
                    let collides = configured.iter().any(|other| {
                        !other.eq_ignore_ascii_case(ign)
                            && other
                                .to_ascii_lowercase()
                                .chars()
                                .take(len)
                                .collect::<String>()
                                == prefix
                    });
                    (!collides).then_some(prefix)
                })
                .unwrap_or_else(|| lower.clone());
            (prefix, ign.clone())
        })
        .collect()
}

pub(super) fn resolve_account(
    value: &str,
    selector: &AccountSelector,
) -> Result<AccountId, RuntimeError> {
    let resolved = selector
        .resolve_running(Some(value))
        .or_else(|| {
            selector
                .configured
                .iter()
                .find(|ign| ign.eq_ignore_ascii_case(value.trim()))
                .cloned()
        })
        .unwrap_or_else(|| value.trim().to_string());
    account_id(resolved, "account")
}

pub(super) fn control_account(selector: &AccountSelector) -> Result<AccountId, RuntimeError> {
    let candidate = selector
        .default_ign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| selector.configured.first().map(String::as_str))
        .or_else(|| selector.running.first().map(String::as_str))
        .unwrap_or_default();
    account_id(candidate, "configured account")
}

pub(super) fn account_id(value: impl Into<String>, label: &str) -> Result<AccountId, RuntimeError> {
    AccountId::new(value).ok_or_else(|| RuntimeError::Invalid(format!("{label} is required.")))
}

pub(super) fn join_command(prefix: &str, message: &str) -> String {
    if message.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} {message}")
    }
}

pub(super) fn required_bank_amount<'a>(
    account: &str,
    request: &'a BankRequest,
) -> Result<&'a str, RuntimeError> {
    let Some(amount) = request
        .amount
        .as_deref()
        .map(str::trim)
        .filter(|amount| !amount.is_empty())
    else {
        return Err(RuntimeError::Invalid(format!(
            "Bank amount is required for {account}. Usage: bank <amount|all> [deposit|withdraw] [personal]"
        )));
    };
    if amount.eq_ignore_ascii_case("all") && request.withdraw {
        return Err(invalid_bank_amount(account, amount));
    }
    if !amount.eq_ignore_ascii_case("all")
        && !parse_compact_number(amount).is_some_and(|value| value.is_finite() && value > 0.0)
    {
        return Err(invalid_bank_amount(account, amount));
    }
    Ok(amount)
}

pub(super) fn invalid_bank_amount(account: &str, amount: &str) -> RuntimeError {
    RuntimeError::Invalid(format!(
        "Invalid bank amount for {account}: {amount}. Usage: bank <amount|all> [deposit|withdraw] [personal]"
    ))
}

pub(super) fn external_buy_action(
    auction_id: &str,
    metadata: Option<AuctionMetadata>,
) -> serde_json::Value {
    let mut action = serde_json::json!({
        "finder": "EXTERNAL",
        "profit": 0,
        "itemName": auction_id,
        "auctionID": auction_id
    });

    if let Some(metadata) = metadata {
        if let Some(item_name) = metadata.item_name {
            action["itemName"] = serde_json::Value::String(item_name);
        }
        if let Some(starting_bid) = metadata.starting_bid {
            action["startingBid"] = serde_json::Value::from(starting_bid);
        }
        if let Some(tag) = metadata.tag {
            action["tag"] = serde_json::Value::String(tag);
        }
    }

    action
}

pub(super) fn tracked_list_flip_action(
    auction_id: &str,
    time_hours: f64,
    tracked: TrackedFlip,
) -> serde_json::Value {
    let item_name = tracked
        .weird_item_name
        .unwrap_or_else(|| auction_id.to_string());
    let mut action = serde_json::json!({
        "auctionID": auction_id,
        "price": tracked.target_price,
        "time": time_hours,
        "weirdItemName": item_name,
        "itemName": item_name,
        "pricePaid": tracked.price_paid.unwrap_or(0.0)
    });

    if let Some(tag) = tracked.tag {
        action["inv"] = serde_json::Value::String(tag.clone());
        action["inventory"] = serde_json::Value::String(tag.clone());
        action["tag"] = serde_json::Value::String(tag);
    }

    action
}

pub(super) fn validate_tracked_list_flip_price(
    auction_id: &str,
    price: f64,
) -> Result<(), RuntimeError> {
    if price.is_finite() && price >= 500.0 {
        Ok(())
    } else {
        Err(tracked_list_flip_error(auction_id))
    }
}

pub(super) fn tracked_list_flip_error(auction_id: &str) -> RuntimeError {
    RuntimeError::Invalid(format!(
        "No valid tracked target price was found for {auction_id}. Provide a manual price."
    ))
}

pub(super) fn inventory_item_is_listable(item: &InventoryItem, include_hotbar: bool) -> bool {
    item.uuid
        .as_ref()
        .is_some_and(|uuid| !uuid.trim().is_empty())
        && item
            .price
            .is_some_and(|price| price.is_finite() && price >= 500.0)
        && (include_hotbar || !item.in_hotbar)
}

pub(super) fn inventory_listing_action(item: &InventoryItem) -> Option<serde_json::Value> {
    let uuid = item
        .uuid
        .as_deref()
        .map(str::trim)
        .filter(|uuid| !uuid.is_empty())?;
    let price = item
        .price
        .filter(|price| price.is_finite() && *price >= 500.0)?;
    let mut action = serde_json::json!({
        "auctionID": uuid,
        "inv": uuid,
        "price": price,
        "weirdItemName": item.item_name.clone(),
        "time": 48
    });

    if let Some(tag) = &item.tag {
        action["tag"] = serde_json::Value::String(tag.clone());
    }

    Some(action)
}

pub(super) fn schedule_account(
    selector: &AccountSelector,
    action: ScheduledAccountAction,
    duration: Option<&str>,
    requested_account: Option<&str>,
) -> Result<RuntimeDirective, RuntimeError> {
    let raw_duration = duration
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RuntimeError::Invalid("Schedule duration is required.".to_string()))?;
    let delay = normal_time(raw_duration).ok_or_else(|| {
        RuntimeError::Invalid(format!("{raw_duration} is not a valid schedule duration."))
    })?;
    if delay.is_zero() {
        return Err(RuntimeError::Invalid(
            "Schedule duration must be greater than zero.".to_string(),
        ));
    }
    let account = requested_account
        .and_then(|requested| resolve_account(requested, selector).ok())
        .or_else(|| {
            selector
                .default_running_ign()
                .or_else(|| selector.default_ign.clone())
                .or_else(|| selector.configured.first().cloned())
                .and_then(AccountId::new)
        })
        .ok_or_else(|| RuntimeError::Invalid("Scheduled account is required.".to_string()))?;

    Ok(RuntimeDirective::ScheduleAccount {
        account,
        action,
        delay_ms: delay.as_millis() as u64,
    })
}

pub(super) fn external_buy_flip(
    account: AccountId,
    message: &str,
) -> Result<RuntimeDirective, RuntimeError> {
    let auction_id = required_arg(message, 0, "auction id")?;
    Ok(RuntimeDirective::ExternalBuy {
        account,
        auction_id: auction_id.to_string(),
    })
}

pub(super) fn queue_list_flip(
    account: AccountId,
    message: &str,
) -> Result<RuntimeDirective, RuntimeError> {
    let auction_id = required_arg(message, 0, "auction id")?;
    let first_optional = optional_arg(message, 1);
    let Some(price_or_time) = first_optional else {
        return Ok(RuntimeDirective::TrackedListFlip {
            account,
            auction_id: auction_id.to_string(),
            time_hours: 48.0,
        });
    };

    if price_or_time == "--time" {
        let time = parse_listing_hours(required_arg(message, 2, "listing duration")?)?;
        return Ok(RuntimeDirective::TrackedListFlip {
            account,
            auction_id: auction_id.to_string(),
            time_hours: time,
        });
    }

    if let Some(time) = price_or_time.strip_prefix("--time=") {
        return Ok(RuntimeDirective::TrackedListFlip {
            account,
            auction_id: auction_id.to_string(),
            time_hours: parse_listing_hours(time)?,
        });
    }

    let price = match parse_price(price_or_time) {
        Ok(price) => price,
        Err(error) if optional_arg(message, 2).is_none() => {
            let time = parse_listing_hours(price_or_time).map_err(|_| error)?;
            return Ok(RuntimeDirective::TrackedListFlip {
                account,
                auction_id: auction_id.to_string(),
                time_hours: time,
            });
        }
        Err(error) => return Err(error),
    };
    let time = parse_listing_hours(optional_arg(message, 2).unwrap_or("48h"))?;
    Ok(RuntimeDirective::QueueState {
        account,
        action: serde_json::json!({
            "auctionID": auction_id,
            "price": price,
            "time": time,
            "weirdItemName": auction_id,
            "pricePaid": 0
        }),
        state: BotState::ListingNoName,
        priority: 4,
    })
}

pub(super) fn queue_list_item(
    account: AccountId,
    message: &str,
) -> Result<RuntimeDirective, RuntimeError> {
    let item_uuid = required_arg(message, 0, "item uuid")?;
    let price = parse_price(required_arg(message, 1, "listing price")?)?;
    let time = parse_listing_hours(optional_arg(message, 2).unwrap_or("48h"))?;
    Ok(RuntimeDirective::QueueState {
        account,
        action: serde_json::json!({
            "auctionID": item_uuid,
            "inv": item_uuid,
            "price": price,
            "time": time
        }),
        state: BotState::ListingNoName,
        priority: 4,
    })
}

pub(super) fn queue_delist(
    account: AccountId,
    message: &str,
) -> Result<RuntimeDirective, RuntimeError> {
    Ok(RuntimeDirective::QueueState {
        account,
        action: serde_json::json!({
            "auctionID": required_arg(message, 0, "auction id")?,
            "itemUuid": required_arg(message, 1, "item uuid")?
        }),
        state: BotState::Delisting,
        priority: 3,
    })
}

fn required_arg<'a>(message: &'a str, index: usize, label: &str) -> Result<&'a str, RuntimeError> {
    optional_arg(message, index)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| RuntimeError::Invalid(format!("{label} is required.")))
}

fn optional_arg(message: &str, index: usize) -> Option<&str> {
    message.split_whitespace().nth(index)
}

fn parse_price(raw: &str) -> Result<f64, RuntimeError> {
    parse_compact_number(raw)
        .filter(|price| *price >= 500.0)
        .ok_or_else(|| {
            RuntimeError::Invalid(format!(
                "{raw} is not a valid listing price of at least 500."
            ))
        })
}

fn parse_listing_hours(raw: &str) -> Result<f64, RuntimeError> {
    normal_time(raw)
        .map(duration_to_hours)
        .ok_or_else(|| RuntimeError::Invalid(format!("{raw} is not a valid listing duration.")))
}

pub(super) fn parse_log_line_count(
    raw: Option<&str>,
    default: usize,
) -> Result<usize, RuntimeError> {
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Ok(default);
    };
    let requested = raw
        .trim()
        .parse::<usize>()
        .map_err(|_| RuntimeError::Invalid(format!("{raw} is not a valid line count.")))?;
    Ok(requested.clamp(5, 80))
}

pub(super) fn unknown_terminal_command_message(command: &str, message: &str) -> String {
    if message.trim().is_empty() {
        format!("Unknown terminal command: {command}")
    } else {
        format!("Unknown terminal command: {command} {}", message.trim())
    }
}
