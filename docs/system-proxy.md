# Managed desktop proxy backend

Paw's per-user backend manages a desktop proxy only after the user explicitly
enables it and Paw has verified its own IPv4 loopback mixed listener. This
backend does not set a shell's proxy environment variables, intercept traffic,
change DNS, or expose a TUN device. Apps must honor desktop proxy settings:
this is not system-wide enforcement.

* **Windows:** the current user's WinINet LAN profile, when policy and query
  results establish that the static settings can be restored safely. It does
  not write WinHTTP, VPN, or dial-up profiles. PAC/automatic discovery enabled,
  inconsistent profile flags, and machine-scoped policy are not yet supported.
  Some clients pick up WinINet changes only after a new session.
* **GNOME on Linux:** the active GNOME session's GSettings/dconf HTTP and HTTPS
  settings. This needs the writable standard schemas, the session D-Bus and
  both gsettings and dconf. Policy-managed defaults, HTTP proxy authentication,
  and existing SOCKS/FTP hosts are rejected conservatively. An active Hyprland
  session without this desktop backend is not supported. The existing bypass
  list is preserved; applications that opt out can still connect directly.

## Ownership and shutdown contract

A desktop caller supplies the same absolute, private per-user application
directory on every launch. Discovery performs read-only capability checks and
must not implicitly activate or restore anything. Call activate only for an
actually ready and owned mixed listener and explicit user consent, outside the
GUI event loop. Do not switch listener/port while activation is in effect. Call
restore before shutting down a listener on the user's behalf, and surface any
failure. If Paw crashed, offer explicit recovery; there is no proxy mutation on
the next normal launch.

The backend holds a per-directory OS file lock during each operation. It saves a
versioned, synced snapshot and its intended result before the first system
write. Restore rechecks the current settings and overwrites only a complete
match of Paw's expected state. If the original state is already present, it
clears the journal but does not write. External changes, partial native writes,
malformed journals and incompatible policy fail closed; Paw retains the
journal and does not blindly overwrite current settings. A user facing a
retained conflict can inspect their operating system proxy settings before
deciding how to recover. The OS APIs do not provide atomic compare-and-set
against concurrent unrelated applications; the backend rechecks immediately
before each native write and verifies results afterward.

The backend unit tests simulate restart and conflict without writing the
running desktop proxy. The backend itself does not add a GUI control.
