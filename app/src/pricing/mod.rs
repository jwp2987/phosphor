use warpui::{Entity, SingletonEntity};

#[derive(Debug, Clone)]
pub struct AddonCreditsOption {
    #[allow(dead_code)] // field: set at construction or persisted
    pub credits: i32,
    #[allow(dead_code)] // field: set at construction or persisted
    pub price_usd_cents: i32,
}

impl AddonCreditsOption {
    #[allow(dead_code)] // used by sibling unused code or shared name
    pub fn rate(&self) -> f32 {
        self.price_usd_cents as f32 / self.credits as f32
    }
}

#[derive(Debug, Clone)]
pub struct PricingInfo {
    #[allow(dead_code)] // field: set at construction or persisted
    pub plans: Vec<PlanPricing>,
    #[allow(dead_code)] // field: set at construction or persisted
    pub addon_credits_options: Vec<AddonCreditsOption>,
}

#[derive(Debug, Clone)]
pub struct PlanPricing {
    #[allow(dead_code)] // field: set at construction or persisted
    pub plan: StripeSubscriptionPlan,
    #[allow(dead_code)] // field: set at construction or persisted
    pub monthly_plan_price_per_month_usd_cents: i32,
    #[allow(dead_code)] // field: set at construction or persisted
    pub yearly_plan_price_per_month_usd_cents: i32,
    #[allow(dead_code)] // field: set at construction or persisted
    pub request_limit: Option<i32>,
    #[allow(dead_code)] // field: set at construction or persisted
    pub max_team_size: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StripeSubscriptionPlan {
    Business,
    Lightspeed,
    Pro,
    Team,
    Turbo,
    Build,
    BuildBusiness,
    BuildMax,
    #[allow(dead_code)] // used by sibling unused code or shared name
    Other(String),
}

/// Global model for server-side pricing information.
///
/// In Zap this is a local no-op stub: the OSS channel has no cloud service
/// pushing pricing data, so `pricing_info` typically stays `None` for the
/// lifetime of the process, and all getters return `None`. The model is
/// temporarily kept for a handful of request-usage and billing compatibility
/// call sites, and can be removed entirely once cloud cleanup is complete.
#[derive(Debug)]
pub struct PricingInfoModel {
    #[allow(dead_code)] // field: set at construction or persisted
    pricing_info: Option<PricingInfo>,
}

impl PricingInfoModel {
    pub fn new() -> Self {
        Self { pricing_info: None }
    }

    #[allow(dead_code)] // used by sibling unused code or shared name
    pub fn addon_credits_options(&self) -> Option<&[AddonCreditsOption]> {
        self.pricing_info
            .as_ref()
            .map(|info| info.addon_credits_options.as_slice())
    }
}

impl Default for PricingInfoModel {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub enum PricingInfoModelEvent {}

impl Entity for PricingInfoModel {
    type Event = PricingInfoModelEvent;
}

impl SingletonEntity for PricingInfoModel {}
