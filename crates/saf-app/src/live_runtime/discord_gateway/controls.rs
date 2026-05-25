use saf_core::AccountId;

pub(super) fn log_controls() -> Vec<serenity::builder::CreateActionRow> {
    vec![serenity::builder::CreateActionRow::Buttons(vec![
        discord_button(
            "saf:logs",
            "Refresh Log",
            serenity::all::ButtonStyle::Primary,
        ),
        discord_button(
            "saf:messages",
            "Recent Messages",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button(
            "saf:dashboard",
            "Dashboard",
            serenity::all::ButtonStyle::Secondary,
        ),
    ])]
}

pub(super) fn dashboard_return_controls() -> Vec<serenity::builder::CreateActionRow> {
    vec![serenity::builder::CreateActionRow::Buttons(vec![
        discord_button(
            "saf:dashboard",
            "Dashboard",
            serenity::all::ButtonStyle::Primary,
        ),
        discord_button("saf:logs", "Logs", serenity::all::ButtonStyle::Secondary),
        discord_button(
            "saf:messages",
            "Messages",
            serenity::all::ButtonStyle::Secondary,
        ),
    ])]
}

pub(super) fn account_action_controls(
    account: &AccountId,
) -> Vec<serenity::builder::CreateActionRow> {
    vec![
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:stats:{account}"),
                "Stats",
                serenity::all::ButtonStyle::Primary,
            ),
            discord_button(
                format!("saf:profit:{account}"),
                "Profit",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:ping:{account}"),
                "Ping",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:queue:{account}"),
                "Queue",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:inventory:{account}"),
                "Inventory",
                serenity::all::ButtonStyle::Secondary,
            ),
        ]),
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:reconcile:{account}"),
                "Reconcile",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:bids:{account}"),
                "Bids",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:sellInventory:{account}:0"),
                "Sell Inventory",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:delistAll:{account}"),
                "Delist All",
                serenity::all::ButtonStyle::Danger,
            ),
            discord_button(
                format!("saf:stop:{account}"),
                "Stop",
                serenity::all::ButtonStyle::Danger,
            ),
        ]),
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                "saf:dashboard",
                "Dashboard",
                serenity::all::ButtonStyle::Primary,
            ),
            discord_button("saf:logs", "Logs", serenity::all::ButtonStyle::Secondary),
            discord_button(
                "saf:messages",
                "Messages",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                "saf:blacklist",
                "Blacklist",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:cofljson:{account}"),
                "Cofl JSON",
                serenity::all::ButtonStyle::Secondary,
            ),
        ]),
    ]
}

pub(super) fn queue_controls(account: &AccountId) -> Vec<serenity::builder::CreateActionRow> {
    vec![serenity::builder::CreateActionRow::Buttons(vec![
        discord_button(
            format!("saf:queue:{account}"),
            "Refresh Queue",
            serenity::all::ButtonStyle::Primary,
        ),
        discord_button(
            format!("saf:reconcile:{account}"),
            "Reconcile",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button(
            format!("saf:inventory:{account}"),
            "Inventory",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button(
            format!("saf:clearData:{account}"),
            "Clear Data",
            serenity::all::ButtonStyle::Danger,
        ),
        discord_button(
            "saf:dashboard",
            "Dashboard",
            serenity::all::ButtonStyle::Secondary,
        ),
    ])]
}

pub(super) fn inventory_controls(account: &AccountId) -> Vec<serenity::builder::CreateActionRow> {
    vec![serenity::builder::CreateActionRow::Buttons(vec![
        discord_button(
            format!("saf:sellInventory:{account}:0"),
            "Sell Inventory",
            serenity::all::ButtonStyle::Danger,
        ),
        discord_button(
            format!("saf:sellInventory:{account}:1"),
            "Sell + Hotbar",
            serenity::all::ButtonStyle::Danger,
        ),
        discord_button(
            format!("saf:account:{account}"),
            "Account",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button(
            format!("saf:queue:{account}"),
            "Queue",
            serenity::all::ButtonStyle::Primary,
        ),
        discord_button(
            "saf:dashboard",
            "Dashboard",
            serenity::all::ButtonStyle::Secondary,
        ),
    ])]
}

pub(super) fn blacklist_controls() -> Vec<serenity::builder::CreateActionRow> {
    vec![serenity::builder::CreateActionRow::Buttons(vec![
        discord_button(
            "saf:blacklist",
            "Refresh Blacklist",
            serenity::all::ButtonStyle::Primary,
        ),
        discord_button(
            "saf:dashboard",
            "Dashboard",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button(
            "saf:messages",
            "Messages",
            serenity::all::ButtonStyle::Secondary,
        ),
        discord_button("saf:logs", "Logs", serenity::all::ButtonStyle::Secondary),
    ])]
}

pub(super) fn discord_button(
    custom_id: impl Into<String>,
    label: impl Into<String>,
    style: serenity::all::ButtonStyle,
) -> serenity::builder::CreateButton {
    serenity::builder::CreateButton::new(custom_id)
        .label(label)
        .style(style)
}
