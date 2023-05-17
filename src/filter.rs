mod imp;

use gstreamer::{glib, prelude::StaticType, Rank};

glib::wrapper! {
  pub struct CoquittsFilter(ObjectSubclass<imp::CoquittsFilter>) @extends gstreamer_base::BaseTransform, gstreamer::Element, gstreamer::Object;
}

pub fn register(plugin: &gstreamer::Plugin) -> Result<(), glib::BoolError> {
  gstreamer::Element::register(
    Some(plugin),
    "coquitts",
    Rank::None,
    CoquittsFilter::static_type(),
  )
}
