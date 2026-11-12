// The Android host (android/) sends the device's FCM token as the host event
// `push.token`; Rust keeps it for cloud mode to register with the server.
import { host } from "nsh";
import { Push } from "nsh-mail/native";

/** Call once at the top level: the host keeps its events until a listener is added. */
export function listenForPushToken() {
  host.on("push.token", (payload) => {
    const token = (payload as { token?: unknown } | null)?.token;
    if (typeof token === "string" && token) void Push.setToken(token);
  });
}
