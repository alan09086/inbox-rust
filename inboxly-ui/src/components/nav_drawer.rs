//! Nav drawer component -- left sidebar with navigation items.

use dioxus::prelude::*;

use crate::app::{Inboxly, Message};
use crate::nav::{NavSection, NavTarget};
use crate::theme::{ActiveView, avatar_colors, category_color};

/// Nav drawer component (264dp wide).
///
/// Contains:
/// - Account switcher header
/// - Primary nav (Inbox, Snoozed, Done)
/// - Secondary nav (Drafts, Sent, Reminders, Trash, Spam)
/// - Bundle category list with coloured dots
#[component]
pub fn NavDrawer() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();

    rsx! {
        nav {
            class: "nav-drawer",

            // Account switcher header
            AccountSwitcherHeader {}

            // Account list (expanded when open)
            {
                let state = app_state.read();
                if state.account_switcher_open {
                    rsx! { AccountList {} }
                } else {
                    rsx! {}
                }
            }

            div { class: "nav-divider" }

            // Primary nav: Inbox, Snoozed, Done
            for view in [ActiveView::Inbox, ActiveView::Snoozed, ActiveView::Done] {
                {
                    let target = NavTarget::View(view);
                    let is_active = app_state.read().active_nav == target;
                    let label = view.title();
                    let target_clone = target.clone();
                    rsx! {
                        button {
                            class: if is_active { "nav-item active" } else { "nav-item" },
                            onclick: move |_| {
                                app_state.write().update(Message::Navigate(target_clone.clone()));
                            },
                            "{label}"
                        }
                    }
                }
            }

            div { class: "nav-divider" }

            // Secondary nav: Drafts, Sent, Reminders, Trash, Spam
            for section in NavSection::all() {
                {
                    let target = NavTarget::Section(*section);
                    let is_active = app_state.read().active_nav == target;
                    let label = section.label();
                    let target_clone = target.clone();
                    rsx! {
                        button {
                            class: if is_active { "nav-item active" } else { "nav-item" },
                            onclick: move |_| {
                                app_state.write().update(Message::Navigate(target_clone.clone()));
                            },
                            "{label}"
                        }
                    }
                }
            }

            div { class: "nav-divider" }

            // Bundle section header
            div { class: "bundle-section-header", "Bundles" }

            // Bundle category list
            div {
                class: "bundle-list",
                {
                    let categories = app_state.read().bundle_categories.clone();
                    rsx! {
                        for cat in categories {
                            {
                                let name = cat.name.clone();
                                let target = NavTarget::BundleCategory(name.clone());
                                let is_active = app_state.read().active_nav == target;
                                let dot_color = category_color(&name).title.to_css();
                                let target_clone = target.clone();
                                rsx! {
                                    button {
                                        class: if is_active { "nav-item active" } else { "nav-item" },
                                        onclick: move |_| {
                                            app_state.write().update(Message::Navigate(target_clone.clone()));
                                        },
                                        span {
                                            class: "nav-dot",
                                            style: "background: {dot_color};",
                                        }
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Account switcher header -- shows active account avatar, name, email.
#[component]
fn AccountSwitcherHeader() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let display_name = state.active_display_name().to_string();
    let email = state.active_email().to_string();
    let first_char = display_name.chars().next().unwrap_or('?');
    let avatar_bg = avatar_colors::for_letter(first_char).to_css();
    let letter = first_char.to_uppercase().to_string();
    let chevron = if state.account_switcher_open {
        "\u{25B2}"
    } else {
        "\u{25BC}"
    };

    drop(state);

    rsx! {
        button {
            class: "account-header",
            onclick: move |_| {
                app_state.write().update(Message::ToggleAccountSwitcher);
            },

            div {
                class: "account-header-avatar",
                style: "background: {avatar_bg};",
                "{letter}"
            }

            div {
                class: "account-info",

                div {
                    class: "account-name-row",
                    span { class: "account-name", "{display_name}" }
                    span { class: "chevron", "{chevron}" }
                }

                span { class: "account-email", "{email}" }
            }
        }
    }
}

/// Expanded account list -- shows all accounts with switch action.
#[component]
fn AccountList() -> Element {
    let mut app_state = use_context::<Signal<Inboxly>>();
    let state = app_state.read();

    let accounts: Vec<_> = state
        .accounts
        .iter()
        .enumerate()
        .map(|(i, acc)| {
            let is_active = i == state.active_account_index;
            let first_char = if acc.display_name.is_empty() {
                acc.email.chars().next().unwrap_or('?')
            } else {
                acc.display_name.chars().next().unwrap_or('?')
            };
            let avatar_bg = avatar_colors::for_letter(first_char).to_css();
            let letter = first_char.to_uppercase().to_string();
            (i, acc.email.clone(), letter, avatar_bg, is_active)
        })
        .collect();

    drop(state);

    rsx! {
        for (index, email, letter, avatar_bg, is_active) in accounts {
            button {
                class: if is_active { "account-row active" } else { "account-row" },
                onclick: move |_| {
                    app_state.write().update(Message::SwitchAccount(index));
                },

                div {
                    class: "account-row-avatar",
                    style: "background: {avatar_bg};",
                    "{letter}"
                }

                span { class: "account-row-email", "{email}" }

                if is_active {
                    span { class: "account-check", "\u{2713}" }
                }
            }
        }

        // Add account button
        button {
            class: "add-account-row",
            onclick: move |_| {
                app_state.write().update(Message::NavigateToSettings);
            },
            span { class: "add-account-plus", "+" }
            "Add account"
        }
    }
}
