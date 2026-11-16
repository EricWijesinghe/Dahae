// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

/// Ticks the three required signup consents on the phone screen. The customer app does not send the
/// code until they are ticked, so every signup flow in a test must agree first.
Future<void> agreeSignupConsents(WidgetTester tester) async {
  for (final kind in ['terms', 'privacy', 'overseas_transfer']) {
    await tester.tap(find.byKey(ValueKey('signup-consent-$kind')));
    await tester.pump();
  }
}
