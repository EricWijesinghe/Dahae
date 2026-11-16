import 'package:console/screens/setup_formats.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const fakeSecret = 'fake-portone-api-secret-000000';
  final cases = <(String, String, String, String?)>[
    ('portone', 'store_id', 'store-00000000-0000-0000-0000-000000000000', null),
    ('portone', 'store_id', 'shop-1', 'invalid_store_id'),
    ('portone', 'store_id', 'store-1 2', 'invalid_store_id'),
    ('portone', 'api_secret', fakeSecret, null),
    ('portone', 'api_secret', 'short', 'invalid_api_secret'),
    ('portone', 'api_secret', 'has a space in the middle', 'invalid_api_secret'),
    ('portone', 'webhook_secret', 'whsec_aGVsbG8=', null),
    ('portone', 'webhook_secret', 'secret-without-prefix', 'invalid_webhook_secret'),
    ('portone', 'channel_keys', 'channel-key-00000000-0000-0000-0000-000000000000', null),
    ('portone', 'channel_keys', 'ch-1', 'invalid_channel_key'),
    ('payout', 'secret_key', 'test_sk_fakefakefakefake', null),
    ('payout', 'secret_key', 'live_sk_fakefakefakefake', null),
    ('payout', 'secret_key', 'sk_fakefakefakefake', 'invalid_secret_key'),
    ('payout', 'security_key', '0123456789abcdef' * 4, null),
    ('payout', 'security_key', '0123456789abcdef', 'invalid_security_key'),
    ('payout', 'security_key', 'g' * 64, 'invalid_security_key'),
    ('portone', 'api_secret', '', null),
    ('payout', 'mode', 'anything', null),
  ];

  for (final (card, field, value, expected) in cases) {
    test('$card.$field "$value" -> ${expected ?? 'ok'}', () {
      expect(formatError(card, field, value), expected);
    });
  }
}
