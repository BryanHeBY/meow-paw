/* Private synthetic org.freedesktop.portal.FileChooser for the Paw X11 smoke. */
#include <gio/gio.h>
#include <stdio.h>

#define SERVICE "org.freedesktop.portal.Desktop"
#define DESKTOP "/org/freedesktop/portal/desktop"
#define CHOOSER "org.freedesktop.portal.FileChooser"
#define REQUEST "org.freedesktop.portal.Request"

static const char xml[] =
    "<node><interface name='" CHOOSER "'>"
    "<property name='version' type='u' access='read'/>"
    "<method name='OpenFile'>"
    "<arg type='s' direction='in'/><arg type='s' direction='in'/>"
    "<arg type='a{sv}' direction='in'/><arg type='o' direction='out'/>"
    "</method></interface></node>";

struct fixture {
    unsigned requests;
    char **local_uris;
};

static void open_file(GDBusConnection *connection, const char *sender,
                      const char *object_path, const char *interface,
                      const char *method, GVariant *params,
                      GDBusMethodInvocation *call, gpointer data)
{
    (void)object_path;
    (void)interface;
    struct fixture *fixture = data;
    const char *parent, *title, *token = NULL, *accept = NULL;
    gboolean modal = FALSE, multiple = FALSE, directory = FALSE;
    GVariant *options;
    g_variant_get(params, "(&s&s@a{sv})", &parent, &title, &options);
    const gboolean valid =
        g_str_equal(method, "OpenFile") &&
        g_str_has_prefix(parent, "x11:") && g_str_equal(title, "Open File") &&
        g_variant_lookup(options, "handle_token", "&s", &token) &&
        g_variant_lookup(options, "accept_label", "&s", &accept) &&
        g_str_equal(accept, "Choose YAML files to import") &&
        g_variant_lookup(options, "modal", "b", &modal) && modal &&
        g_variant_lookup(options, "multiple", "b", &multiple) && multiple &&
        (!g_variant_lookup(options, "directory", "b", &directory) || !directory);
    if (!valid || !g_dbus_is_unique_name(sender) || fixture->requests >= 3) {
        g_dbus_method_invocation_return_dbus_error(call, CHOOSER ".Unexpected",
                                                   "Unexpected chooser request");
        g_printerr("Unexpected chooser request #%u: %s (%s; %s)\n",
                   fixture->requests + 1, method, parent, title);
        g_variant_unref(options);
        return;
    }

    g_autofree char *client = g_strdup(sender + 1);
    g_strdelimit(client, ".", '_');
    g_autofree char *path = g_strdup_printf(DESKTOP "/request/%s/%s", client, token);
    g_assert(g_variant_is_object_path(path));
    g_variant_unref(options);

    const char *nonlocal[] = { "https://example.invalid/not-local.yaml", NULL };
    const char *const *uris = NULL;
    unsigned code = 0;
    const char *kind;
    switch (++fixture->requests) {
    case 1: code = 1; kind = "cancel"; break;
    case 2: uris = nonlocal; kind = "non-local"; break;
    default: uris = (const char *const *)fixture->local_uris; kind = "local"; break;
    }
    GVariantBuilder result;
    g_variant_builder_init(&result, G_VARIANT_TYPE_VARDICT);
    if (uris) {
        g_variant_builder_add(&result, "{sv}", "uris", g_variant_new_strv(uris, -1));
    }

    /* ashpd subscribes before making OpenFile; queue the reply before Response. */
    g_dbus_method_invocation_return_value(call, g_variant_new("(o)", path));
    g_autoptr(GError) error = NULL;
    if (!g_dbus_connection_emit_signal(connection, sender, path, REQUEST, "Response",
                                       g_variant_new("(ua{sv})", code, &result), &error)) {
        g_error("Could not emit Response: %s", error->message);
    }
    g_print("OpenFile %u %s %s\n", fixture->requests, kind, path);
    fflush(stdout);
}

static GVariant *property(GDBusConnection *connection, const char *sender,
                          const char *path, const char *interface,
                          const char *name, GError **error, gpointer data)
{
    (void)connection;
    (void)sender;
    (void)path;
    (void)interface;
    (void)error;
    (void)data;
    g_assert(g_str_equal(name, "version"));
    return g_variant_new_uint32(4);
}

static const GDBusInterfaceVTable chooser = { .method_call = open_file, .get_property = property };

static gboolean private_fixture(int argc, char **argv)
{
    const char *home = g_getenv("HOME");
    if (argc != 3 || !home) return FALSE;
    g_autofree char *base = g_path_get_basename(home);
    g_autofree char *runtime = g_build_filename(home, "runtime", NULL);
    g_autofree char *address = g_strdup_printf("unix:path=%s/bus", runtime);
    if (!g_str_has_prefix(base, "paw-picker-") ||
        g_strcmp0(g_getenv("XDG_RUNTIME_DIR"), runtime) ||
        g_strcmp0(g_getenv("DBUS_SESSION_BUS_ADDRESS"), address)) return FALSE;

    g_autofree char *prefix = g_strconcat(home, "/", NULL);
    for (int i = 1; i < argc; i++) {
        g_autofree char *file = g_filename_from_uri(argv[i], NULL, NULL);
        if (!file || !g_str_has_prefix(file, prefix) ||
            !g_file_test(file, G_FILE_TEST_IS_REGULAR)) return FALSE;
    }
    return TRUE;
}

int main(int argc, char **argv)
{
    if (!private_fixture(argc, argv)) {
        g_printerr("Use the private fixture runner with two synthetic file URIs.\n");
        return 2;
    }
    struct fixture fixture = { .local_uris = argv + 1 };
    g_autoptr(GError) error = NULL;
    g_autoptr(GDBusConnection) bus = g_bus_get_sync(G_BUS_TYPE_SESSION, NULL, &error);
    if (!bus) g_error("Cannot connect to test bus: %s", error->message);
    g_autoptr(GDBusNodeInfo) node = g_dbus_node_info_new_for_xml(xml, &error);
    if (!node) g_error("Invalid portal fixture introspection: %s", error->message);
    const guint registered = g_dbus_connection_register_object(bus, DESKTOP, node->interfaces[0],
                                                               &chooser, &fixture, NULL, &error);
    if (!registered) g_error("Cannot register chooser: %s", error->message);
    g_autoptr(GVariant) ownership = g_dbus_connection_call_sync(
        bus, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
        "RequestName", g_variant_new("(su)", SERVICE, 4), G_VARIANT_TYPE("(u)"),
        G_DBUS_CALL_FLAGS_NONE, 2000, NULL, &error);
    if (!ownership) g_error("Cannot own synthetic portal name: %s", error->message);
    unsigned acquired;
    g_variant_get(ownership, "(u)", &acquired);
    g_assert_cmpuint(acquired, ==, 1);
    g_print("READY\n");
    fflush(stdout);
    g_autoptr(GMainLoop) loop = g_main_loop_new(NULL, FALSE);
    g_main_loop_run(loop);
    return 0;
}
