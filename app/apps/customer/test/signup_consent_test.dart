import 'package:customer/screens/signup_consent.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('nothing is agreed until every required consent is ticked', () {
    final granted = emptySignupConsents();
    expect(signupRequiredAgreed(granted), isFalse);

    granted['terms'] = true;
    granted['privacy'] = true;
    expect(
      signupRequiredAgreed(granted),
      isFalse,
      reason: 'overseas transfer is still missing',
    );

    granted['overseas_transfer'] = true;
    expect(signupRequiredAgreed(granted), isTrue);
  });

  test('marketing is optional and does not gate the code', () {
    final granted = emptySignupConsents()
      ..['terms'] = true
      ..['privacy'] = true
      ..['overseas_transfer'] = true;
    expect(granted['marketing'], isFalse);
    expect(signupRequiredAgreed(granted), isTrue);
  });

  test(
    'payload names every kind with its answer and the policy version shown',
    () {
      final granted = emptySignupConsents()..['terms'] = true;
      final payload = signupConsentPayload(granted);

      expect(payload.map((c) => c['kind']), [
        'terms',
        'privacy',
        'overseas_transfer',
        'marketing',
      ]);
      expect(payload.first, {
        'kind': 'terms',
        'granted': true,
        'version': signupConsentVersion,
      });
      expect(payload.skip(1).every((c) => c['granted'] == false), isTrue);
      expect(
        payload.every((c) => c['version'] == signupConsentVersion),
        isTrue,
      );
    },
  );
}
