# Local X11 drag-and-drop patch
This directory contains the crates.io gpui-pre-linux 0.3.14 release
(SHA-256 9515d52d8378c5dda95b3b2536d3d05ea2431bc25d1d8bb4cdeb32f21090f4a1),
from the Zed GPUI snapshot used by gpui-kit 0.11.0.
Upstream repository: https://github.com/zed-industries/zed
Package: https://crates.io/crates/gpui-pre-linux/0.3.14
The upstream Apache License 2.0 text is retained as LICENSE-APACHE.

Paw changes src/linux/x11/client.rs only: prefer text/uri-list when a drag
source also offers a plain text representation. Preserve text-only drop
selection, and cover both inline and property-backed XDND offers.
This override must be reconsidered on gpui-kit upgrades and removed when
the pinned dependency implements the same behavior.
