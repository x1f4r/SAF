use azalea_inventory::{ItemStack as AzaleaItemStack, components};
use simdnbt::owned::NbtCompound;

pub(super) fn minecraft_item_name(debug_name: &str) -> String {
    let mut out = String::new();
    for (index, ch) in debug_name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AzaleaItemMetadata {
    pub(super) item_name: String,
    pub(super) display_name: String,
    pub(super) lore: Vec<String>,
    pub(super) item_uuid: Option<String>,
    pub(super) skyblock_tag: Option<String>,
}

pub(super) fn azalea_item_metadata(item: &AzaleaItemStack) -> AzaleaItemMetadata {
    let kind_debug = format!("{:?}", item.kind());
    let item_name = minecraft_item_name(&kind_debug);
    let display_name = component_text::<components::CustomName>(item)
        .or_else(|| component_text::<components::ItemName>(item))
        .unwrap_or_else(|| kind_debug.clone());
    let lore = item
        .get_component::<components::Lore>()
        .map(|lore| lore.lines.iter().map(ToString::to_string).collect())
        .unwrap_or_default();
    let (item_uuid, skyblock_tag) = item
        .get_component::<components::CustomData>()
        .map(|custom_data| extra_attribute_ids(&custom_data))
        .unwrap_or_default();

    AzaleaItemMetadata {
        item_name,
        display_name,
        lore,
        item_uuid,
        skyblock_tag,
    }
}

trait NamedComponent {
    fn text(&self) -> String;
}

impl NamedComponent for components::CustomName {
    fn text(&self) -> String {
        self.name.to_string()
    }
}

impl NamedComponent for components::ItemName {
    fn text(&self) -> String {
        self.name.to_string()
    }
}

fn component_text<T>(item: &AzaleaItemStack) -> Option<String>
where
    T: components::DataComponentTrait + NamedComponent,
{
    item.get_component::<T>()
        .map(|component| component.text())
        .filter(|text| !text.trim().is_empty())
}

fn extra_attribute_ids(custom_data: &components::CustomData) -> (Option<String>, Option<String>) {
    let Some(extra) = extra_attributes(custom_data) else {
        return (None, None);
    };
    let tag = skyblock_tag(extra);
    let uuid = nbt_string(extra, "uuid");
    (uuid, tag)
}

fn extra_attributes(custom_data: &components::CustomData) -> Option<&NbtCompound> {
    custom_data
        .nbt
        .compound("ExtraAttributes")
        .or_else(|| {
            custom_data
                .nbt
                .compound("tag")
                .and_then(|tag| tag.compound("ExtraAttributes"))
        })
        .or_else(|| {
            let root: &NbtCompound = &custom_data.nbt;
            (root.string("uuid").is_some() || root.string("id").is_some()).then_some(root)
        })
}

fn skyblock_tag(extra: &NbtCompound) -> Option<String> {
    let id = nbt_string(extra, "id")?;
    let first = id.split('_').next().unwrap_or_default();
    if matches!(first, "RUNE" | "UNIQUE")
        && let Some(rune) = extra
            .compound("runes")
            .and_then(|runes| runes.keys().next())
            .map(ToString::to_string)
    {
        return Some(format!("{rune}_RUNE"));
    }
    Some(id)
}

fn nbt_string(compound: &NbtCompound, key: &str) -> Option<String> {
    compound.string(key).map(ToString::to_string)
}
