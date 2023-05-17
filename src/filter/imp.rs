use std::{str, sync::Mutex};

use byte_slice_cast::AsByteSlice;
use gstreamer::{
  caps::NoFeature,
  glib::{self, ParamSpec, Value},
  param_spec::GstParamSpecBuilderExt,
  prelude::{ParamSpecBuilderExt, ToValue},
  subclass::{
    prelude::{ElementImpl, GstObjectImpl, ObjectImpl, ObjectSubclass},
    ElementMetadata,
  },
  Buffer, Caps, CapsIntersectMode, DebugCategory, ErrorMessage, FlowError, PadDirection,
  PadPresence, PadTemplate,
};
use gstreamer_audio::{AudioCapsBuilder, AUDIO_FORMAT_F32};
use gstreamer_base::{
  subclass::{
    base_transform::{BaseTransformImpl, BaseTransformImplExt, GenerateOutputSuccess},
    BaseTransformMode,
  },
  BaseTransform,
};
use once_cell::sync::Lazy;
use pyo3::{
  types::{PyBool, PyDict, PyList, PyModule},
  Py, PyAny, Python,
};

const DEFAULT_MODEL: &str = "tts_models/tr/common-voice/glow-tts";
const DEFAULT_GPU: bool = false;

static CAT: Lazy<DebugCategory> = Lazy::new(|| {
  DebugCategory::new(
    "coquitts",
    gstreamer::DebugColorFlags::empty(),
    Some("Text to speech filter using Coqui"),
  )
});

fn src_caps_builder() -> AudioCapsBuilder<NoFeature> {
  AudioCapsBuilder::new().format(AUDIO_FORMAT_F32).channels(1)
}

static SRC_CAPS: Lazy<Caps> = Lazy::new(|| src_caps_builder().build());

static SINK_CAPS: Lazy<Caps> =
  Lazy::new(|| Caps::builder("text/x-raw").field("format", "utf8").build());

#[derive(Debug, Clone, Default)]
struct Settings {
  model: String,
  speaker: Option<String>,
  language: Option<String>,
  voice_cloning_input_file: Option<String>,
  gpu: bool,
}

pub struct CoquittsFilter {
  #[allow(dead_code)]
  settings: Mutex<Settings>,
  synth: Mutex<Option<Py<PyAny>>>,
}

#[glib::object_subclass]
impl ObjectSubclass for CoquittsFilter {
  type ParentType = BaseTransform;
  type Type = super::CoquittsFilter;

  const NAME: &'static str = "GstCoquittsFilter";

  fn new() -> Self {
    Self {
      settings: Mutex::new(Settings {
        model: DEFAULT_MODEL.into(),
        speaker: None,
        language: None,
        voice_cloning_input_file: None,
        gpu: DEFAULT_GPU,
      }),
      synth: Mutex::new(None),
    }
  }
}

impl ObjectImpl for CoquittsFilter {
  fn properties() -> &'static [ParamSpec] {
    static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
      vec![
      glib::ParamSpecString::builder("model")
        .nick("Model")
        .blurb(&format!("The Coqui TTS model to use. Defaults to {}. Possible values can be listed with `tts --list_models`", DEFAULT_MODEL))
        .mutable_ready()
        .build(),
      glib::ParamSpecString::builder("speaker")
        .nick("Speaker")
        .blurb("The speaker name to use, for multi-speaker models.")
        .mutable_ready()
        .build(),
      glib::ParamSpecString::builder("language")
        .nick("Language")
        .blurb("The language identifier to use, for multi-language models.")
        .mutable_ready()
        .build(),
      glib::ParamSpecString::builder("voice-cloning-input-file")
        .nick("Voice Cloning input file")
        .blurb("A WAV file to clone the voice from, for models that support voice cloning.")
        .mutable_ready()
        .build(),
      glib::ParamSpecBoolean::builder("use-gpu")
        .nick("Use GPU")
        .blurb(&format!("Whether to use the GPU. Defaults to {}", DEFAULT_GPU))
        .mutable_ready()
        .build(),
    ]
    });
    PROPERTIES.as_ref()
  }

  fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
    let mut settings = self.settings.lock().unwrap();
    match pspec.name() {
      "model" => {
        settings.model = value.get().unwrap();
      },
      "speaker" => {
        settings.speaker = value.get().unwrap();
      },
      "language" => {
        settings.language = value.get().unwrap();
      },
      "voice-cloning-input-file" => {
        settings.voice_cloning_input_file = value.get().unwrap();
      },
      "use-gpu" => {
        settings.gpu = value.get().unwrap();
      },
      other => panic!("no such property: {}", other),
    }
  }

  fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
    let settings = self.settings.lock().unwrap();
    match pspec.name() {
      "model" => settings.model.to_value(),
      "speaker" => settings.speaker.to_value(),
      "language" => settings.language.to_value(),
      "voice-cloning-input-file" => settings.voice_cloning_input_file.to_value(),
      "use-gpu" => settings.gpu.to_value(),
      other => panic!("no such property: {}", other),
    }
  }
}

impl GstObjectImpl for CoquittsFilter {}

impl ElementImpl for CoquittsFilter {
  fn metadata() -> Option<&'static ElementMetadata> {
    static ELEMENT_METADATA: Lazy<ElementMetadata> = Lazy::new(|| {
      ElementMetadata::new(
        "Coqui TTS",
        "Converter/Text/Audio",
        "Text to speech filter using Coqui",
        "Jasper Hugo <jasper@avstack.io>",
      )
    });

    Some(&*ELEMENT_METADATA)
  }

  fn pad_templates() -> &'static [PadTemplate] {
    static PAD_TEMPLATES: Lazy<Vec<PadTemplate>> = Lazy::new(|| {
      let src_pad_template =
        PadTemplate::new("src", PadDirection::Src, PadPresence::Always, &SRC_CAPS).unwrap();

      let sink_pad_template = gstreamer::PadTemplate::new(
        "sink",
        gstreamer::PadDirection::Sink,
        gstreamer::PadPresence::Always,
        &SINK_CAPS,
      )
      .unwrap();

      vec![src_pad_template, sink_pad_template]
    });

    PAD_TEMPLATES.as_ref()
  }
}

impl CoquittsFilter {
  fn init_synth(&self) -> Py<PyAny> {
    gstreamer::debug!(CAT, "init_synth(): initialising Python interpreter");
    pyo3::prepare_freethreaded_python();
    gstreamer::debug!(CAT, "init_synth(): acquiring GIL");
    let result = Python::with_gil(|py| {
      gstreamer::debug!(CAT, "init_synth(): init synth");
      let tts_api_module = PyModule::import(py, "TTS.api").unwrap();
      let kwargs = {
        let settings = self.settings.lock().unwrap();
        let d = PyDict::new(py);
        d.set_item("model_name", &settings.model).unwrap();
        d.set_item("progress_bar", false).unwrap();
        d.set_item("gpu", settings.gpu).unwrap();
        d
      };
      let synth = tts_api_module.call_method("TTS", (), Some(kwargs)).unwrap();
      gstreamer::debug!(CAT, "init_synth(): synth init complete");
      {
        let settings = self.settings.lock().unwrap();
        if settings.language.is_none()
          && synth
            .getattr("is_multi_lingual")
            .unwrap()
            .downcast::<PyBool>()
            .unwrap()
            .is_true()
        {
          panic!("This model is multi-lingual and requires specifying the `language` property");
        }
        if settings.speaker.is_none()
          && synth
            .getattr("is_multi_speaker")
            .unwrap()
            .downcast::<PyBool>()
            .unwrap()
            .is_true()
        {
          panic!("This model is multi-speaker and requires specifying the `speaker` property");
        }
      }
      synth.into()
    });
    gstreamer::debug!(CAT, "init_synth(): released GIL");
    result
  }

  fn with_synth<R, F: FnOnce(&PyAny) -> R>(&self, f: F) -> R {
    gstreamer::debug!(CAT, "with_synth(): locking synth");
    let mut synth = self.synth.lock().unwrap();
    if synth.is_none() {
      gstreamer::debug!(CAT, "with_synth(): no synth, will init");
      *synth = Some(self.init_synth());
    }
    gstreamer::debug!(CAT, "with_synth(): acquiring GIL");
    let result = Python::with_gil(move |py| {
      let result = f(synth.as_ref().unwrap().as_ref(py));
      drop(synth);
      gstreamer::debug!(CAT, "with_synth(): unlocked synth");
      result
    });
    gstreamer::debug!(CAT, "with_synth(): released GIL");
    result
  }
}

impl BaseTransformImpl for CoquittsFilter {
  const MODE: BaseTransformMode = BaseTransformMode::NeverInPlace;
  const PASSTHROUGH_ON_SAME_CAPS: bool = false;
  const TRANSFORM_IP_ON_PASSTHROUGH: bool = false;

  fn start(&self) -> Result<(), ErrorMessage> {
    gstreamer::debug!(CAT, "start()");
    Ok(())
  }

  fn stop(&self) -> Result<(), ErrorMessage> {
    gstreamer::debug!(CAT, "stop()");
    Ok(())
  }

  fn transform_caps(
    &self,
    direction: PadDirection,
    _caps: &Caps,
    maybe_filter: Option<&Caps>,
  ) -> Option<Caps> {
    let mut caps = if direction == PadDirection::Src {
      SINK_CAPS.clone()
    }
    else {
      let sample_rate = self.with_synth(|s| {
        s.getattr("synthesizer")
          .unwrap()
          .getattr("output_sample_rate")
          .unwrap()
          .extract::<u64>()
          .unwrap()
      });
      gstreamer::debug!(CAT, "transform_caps(): using sample rate: {}", sample_rate);
      src_caps_builder().rate(sample_rate as i32).build()
    };
    if let Some(filter) = maybe_filter {
      caps = filter.intersect_with_mode(&caps, CapsIntersectMode::First);
    }
    Some(caps)
  }

  fn generate_output(&self) -> Result<GenerateOutputSuccess, FlowError> {
    if let Some(buffer) = self.take_queued_buffer() {
      let buffer_reader = buffer
        .as_ref()
        .map_readable()
        .map_err(|_| FlowError::Error)?;
      let text = str::from_utf8(buffer_reader.as_slice()).map_err(|_| FlowError::Error)?;
      gstreamer::debug!(CAT, "generate_output(): synthesising: {}", text);
      let maybe_audio = self.with_synth(|s| {
        let kwargs = {
          let settings = self.settings.lock().unwrap();
          let d = PyDict::new(s.py());
          d.set_item("text", text).unwrap();
          if let Some(speaker) = settings.speaker.as_ref() {
            d.set_item("speaker", speaker).unwrap();
          }
          if let Some(language) = settings.language.as_ref() {
            d.set_item("language", language).unwrap();
          }
          if let Some(file) = settings.voice_cloning_input_file.as_ref() {
            d.set_item("speaker_wav", file).unwrap();
          }
          d
        };
        match s.call_method("tts", (), Some(kwargs)) {
          Ok(any) => Some(
            any
              .downcast::<PyList>()
              .unwrap()
              .extract::<Vec<f32>>()
              .unwrap(),
          ),
          Err(e) => {
            gstreamer::debug!(
              CAT,
              "generate_output(): failed to synthesise samples: {:?}",
              e
            );
            e.print(s.py());
            None
          },
        }
      });
      if let Some(audio) = maybe_audio {
        gstreamer::debug!(
          CAT,
          "generate_output(): synthesised {} samples",
          audio.len()
        );
        gstreamer::debug!(
          CAT,
          "generate_output(): first 32 samples: {:?}",
          &audio[..32]
        );
        let audio_bytes = audio.as_byte_slice();
        gstreamer::debug!(
          CAT,
          "generate_output(): synthesised {} bytes",
          audio_bytes.len()
        );
        let mut buffer = Buffer::with_size(audio_bytes.len()).map_err(|_| FlowError::Error)?;
        buffer
          .get_mut()
          .unwrap()
          .copy_from_slice(0, audio_bytes)
          .map_err(|_| FlowError::Error)?;
        Ok(GenerateOutputSuccess::Buffer(buffer))
      }
      else {
        Ok(GenerateOutputSuccess::NoOutput)
      }
    }
    else {
      gstreamer::debug!(CAT, "generate_output(): no queued buffers to take");
      Ok(GenerateOutputSuccess::NoOutput)
    }
  }
}
