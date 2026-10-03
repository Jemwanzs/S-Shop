# WhatsApp (Meta WhatsApp Cloud API)

Without configuration every WhatsApp button opens a pre-filled chat (`wa.me` link) on the staff member's phone. With
the Cloud API configured, S'Shop sends messages itself.

## What is sent

| Message | Trigger | Setting |
|---|---|---|
| Order received + tracking link | Portal order placed | Orders → notify customers |
| Order status updates | Each status change | Orders → notify customers |
| Ordering-link verification code | Customer identifies (when OTP is on) | Orders → verify with a WhatsApp code |
| Receipt | Every sale with a customer / *Share* button | Integrations → receipts |
| Credit reminder | Overdue (once) / *Remind* button | Integrations → credit reminders |
| Award winner message | Closing an award period / *WhatsApp* button | Integrations → award messages |
| Auto-reply with order status | Customer texts an order number or “status” | always (when configured) |

## Configuration

| Variable | Notes |
|---|---|
| `WHATSAPP_TOKEN` | Permanent system-user access token |
| `WHATSAPP_PHONE_NUMBER_ID` | From WhatsApp Manager |
| `WHATSAPP_VERIFY_TOKEN` | Random string you also enter in the Meta webhook settings |
| `WHATSAPP_APP_SECRET` | Meta app secret — enables `X-Hub-Signature-256` verification (recommended) |
| `WHATSAPP_NOTIFICATION_TEMPLATE` | Approved **utility** template with one body variable `{{1}}` (e.g. `sshop_notification`) |
| `WHATSAPP_TEMPLATE_LANGUAGE` | Template language code, default `en` |
| `WHATSAPP_API_VERSION` | Graph API version, default `v21.0` |

**Why the template matters:** WhatsApp only delivers free-form text inside the 24-hour window after the customer last
wrote to you. Business-initiated messages (orders, receipts, reminders, codes) are sent through the notification
template when it is set; the message text is placed in `{{1}}` (line breaks become “ · ”). Replies to customers'
messages are free-form.

Webhook (Meta → WhatsApp → Configuration): callback URL `https://<your-domain>/api/webhooks/whatsapp`, verify token =
`WHATSAPP_VERIFY_TOKEN`, subscribe to **messages**. Delivery statuses and inbound messages are stored in
`whatsapp_messages`.
