# Lucent

Lucent is a notification daemon for Wayland desktops: apps send it Notifications over the freedesktop protocol, and it shows them as Popups and remembers them in History.

## Language

**Notification**:
A message an app sends to Lucent, with a summary, optional body, icon, Actions and hints. Identified by a numeric id that a later Notification may reuse to replace it.
_Avoid_: Alert, message, toast

**Popup**:
The on-screen card that shows one Notification.
_Avoid_: Toast, bubble, window

**History**:
Lucent's in-memory record of received Notifications, browsable and restorable from the command line. A replaced Notification keeps a single entry; Transient Notifications are never recorded.
_Avoid_: Log, notification center, archive

**Do Not Disturb**:
A mode in which incoming Notifications go straight to History without showing a Popup, except Critical ones.
_Avoid_: DND mode, silent mode, focus mode

**Urgency**:
How important a Notification is, as set by the sending app: low, normal, or critical.
_Avoid_: Priority, severity

**Critical**:
A Notification whose Urgency is critical; it is allowed through Do Not Disturb.

**Transient**:
A Notification the app has marked as not worth remembering, such as a volume or brightness indicator.
_Avoid_: Ephemeral, temporary

**Action**:
An app-defined choice attached to a Notification, shown as a button on its Popup.
_Avoid_: Button, callback

**Rule**:
A user-defined match on a Notification's app name and/or summary that changes how it is handled (its timeout, Urgency, whether it shows a Popup, whether it enters History).
_Avoid_: Filter, criteria, override

**Overflow**:
Notifications waiting for a free Popup slot because the visible stack is full.
_Avoid_: Queue, pending, backlog

**Theme**:
A named, built-in visual style for Popups.
_Avoid_: Skin, preset
