# gst-coquitts: A GStreamer element that does text-to-speech using Coqui.

Accepts text buffers on its sink pad, does text-to-speech using Coqui, and produces audio buffers on its source pad.

Example usage:

```
gst-launch-1.0 --quiet fdsrc ! 'text/x-raw,format=utf8' ! coquitts ! autoaudiosink
```

## License

gst-coquitts is licensed under either of

* Apache License, Version 2.0, (LICENSE-APACHE or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license (LICENSE-MIT or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Any kinds of contributions are welcome as a pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in these crates by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Acknowledgements

gst-coquitts development is sponsored by [AVStack](https://avstack.io/). We provide globally-distributed, scalable, managed Jitsi Meet backends.
