import 'dart:convert';

/// Client-side format checks for payment keys. They mirror the server (`integrations::apply`), which stays
/// authoritative. An empty value returns null: the server reports missing fields.
///
/// Returns the server's error code for the first rule the value breaks, so [setupMessage] can word it.
String? formatError(String card, String field, String value) {
  final v = value.trim();
  if (v.isEmpty) return null;
  final ok = switch ((card, field)) {
    ('portone', 'store_id') => v.startsWith('store-') && !_space.hasMatch(v),
    ('portone', 'api_secret') =>
      !_space.hasMatch(v) && v.length >= 16 && v.length <= 200,
    ('portone', 'webhook_secret') => _webhookOk(v),
    ('portone', 'channel_keys') =>
      v.startsWith('channel-key-') && !_space.hasMatch(v),
    ('payout', 'secret_key') => _tossSecret.hasMatch(v),
    ('payout', 'security_key') => _hex64.hasMatch(v),
    _ => true,
  };
  if (ok) return null;
  return switch (field) {
    'store_id' => 'invalid_store_id',
    'api_secret' => 'invalid_api_secret',
    'webhook_secret' => 'invalid_webhook_secret',
    'channel_keys' => 'invalid_channel_key',
    'secret_key' => 'invalid_secret_key',
    _ => 'invalid_security_key',
  };
}

final _space = RegExp(r'\s');

/// The server decodes the part after `whsec_` as padded base64, so the same check runs here.
bool _webhookOk(String v) {
  if (!v.startsWith('whsec_')) return false;
  try {
    base64.decode(v.substring('whsec_'.length));
    return true;
  } on FormatException {
    return false;
  }
}

final _tossSecret = RegExp(r'^(test|live)_sk_\S+$');
final _hex64 = RegExp(r'^[0-9a-fA-F]{64}$');
