/* X11 integration-test source. Connect only to the fixture's isolated Xvfb. */
#include <X11/Xlib.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void send_message(Display *d, Window dest, Atom type,
                         long a, long b, long c, long e, long f) {
    XEvent x = {0};
    x.xclient = (XClientMessageEvent){.type = ClientMessage, .display = d, .window = dest,
        .message_type = type, .format = 32, .data.l = {a, b, c, e, f}};
    if (!XSendEvent(d, dest, False, NoEventMask, &x)) exit(2);
    XFlush(d);
}

int main(int argc, char **argv) {
    if (argc != 7 || !getenv("PAW_TEST_XVFB") || !getenv("DISPLAY")
        || strcmp(getenv("PAW_TEST_XVFB"), getenv("DISPLAY"))) return 2;
    const Window dst = strtoul(argv[1], NULL, 0);
    const unsigned long x = strtoul(argv[2], NULL, 10);
    const unsigned long y = strtoul(argv[3], NULL, 10);
    const int plain_first = !strcmp(argv[4], "plain-first");
    const int via_property = !strcmp(argv[4], "property-first");
    const int text_only = !strcmp(argv[4], "text-only");
    if (!plain_first && !via_property && !text_only
        && strcmp(argv[4], "uri-first") && strcmp(argv[4], "invalid-uri")) return 2;
    if (x > 32767 || y > 32767 || dst == None) return 2;
    Display *d = XOpenDisplay(NULL);
    if (!d) return 2;
    Window src = XCreateSimpleWindow(d, DefaultRootWindow(d), 0, 0, 1, 1, 0, 0, 0);
    Atom selection = XInternAtom(d, "XdndSelection", False);
    Atom uri = XInternAtom(d, "text/uri-list", False);
    Atom plain = XInternAtom(d, "UTF8_STRING", False);
    Atom enter = XInternAtom(d, "XdndEnter", False);
    Atom position = XInternAtom(d, "XdndPosition", False);
    Atom status = XInternAtom(d, "XdndStatus", False);
    Atom drop = XInternAtom(d, "XdndDrop", False);
    Atom finished = XInternAtom(d, "XdndFinished", False);
    Atom copy = XInternAtom(d, "XdndActionCopy", False);

    XSetSelectionOwner(d, selection, src, CurrentTime);
    if (XGetSelectionOwner(d, selection) != src) return 2;
    if (via_property) {
        Atom type_list = XInternAtom(d, "XdndTypeList", False);
        Atom types[] = {plain, XInternAtom(d, "application/octet-stream", False),
                        XInternAtom(d, "text/html", False), uri};
        XChangeProperty(d, src, type_list, 4, 32, PropModeReplace,
                        (const unsigned char *)types, 4);
    }
    send_message(d, dst, enter, src, (5L << 24) | via_property,
                 text_only || plain_first ? plain : uri,
                 text_only ? None : plain_first ? uri : plain, None);
    send_message(d, dst, position, src, 0, (long)((x << 16) | y), CurrentTime, copy);
    int dropped = 0, received = 0;
    for (int i = 0; i < 64; ++i) {
        struct pollfd ready = {.fd = ConnectionNumber(d), .events = POLLIN};
        if (!XPending(d) && poll(&ready, 1, 2000) <= 0) break;
        XEvent ev;
        XNextEvent(d, &ev);
        if (ev.type == SelectionRequest) {
            XSelectionRequestEvent req = ev.xselectionrequest;
            XEvent reply = {0};
            Atom property = req.property ? req.property : req.target;
            int valid = req.target == uri || req.target == plain;
            const char *payload = req.target == uri ? argv[5] : argv[6];
            received += valid;
            if (valid) XChangeProperty(d, req.requestor, property, req.target, 8,
                PropModeReplace, (const unsigned char *)payload, (int)strlen(payload));
            reply.xselection = (XSelectionEvent){.type = SelectionNotify, .display = d,
                .requestor = req.requestor, .selection = req.selection, .target = req.target,
                .property = valid ? property : None, .time = req.time};
            printf("selection=%s\n", req.target == uri ? "uri-list" : req.target == plain ? "plain" : "other");
            XSendEvent(d, req.requestor, False, NoEventMask, &reply);
            XFlush(d);
        } else if (ev.type == ClientMessage && ev.xclient.message_type == status && !dropped) {
            int accepted = !!(ev.xclient.data.l[1] & 1);
            printf("accepted=%d\n", accepted);
            if (!accepted) return 3;
            dropped = 1;
            send_message(d, dst, drop, src, 0, CurrentTime, 0, 0);
        } else if (ev.type == ClientMessage && ev.xclient.message_type == finished) {
            printf("finished=%d\n", !!(ev.xclient.data.l[1] & 1));
            XCloseDisplay(d);
            return dropped && received ? 0 : 3;
        }
    }
    fprintf(stderr, "X11 drag/drop handshake timed out\n");
    XCloseDisplay(d);
    return 3;
}
