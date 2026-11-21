// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images of the customer app (phone size, light theme), with the store-shot sample data.
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:customer/screens/booking_detail.dart';
import 'package:customer/screens/booking_flow.dart';
import 'package:customer/screens/chat.dart';
import 'package:customer/screens/home.dart';
import 'package:customer/screens/more.dart';
import 'package:customer/screens/quote_flow.dart';
import 'package:customer/screens/service.dart';
import 'package:customer/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import '../store_screenshots/customer_store_shots_test.dart'
    show storeDraftScreen, storeShotEnv;
import '../support/customer_host.dart';

const _phone = ShotDevice('phone', 780, 1688, 2);
final _out = const String.fromEnvironment(
  'SHOT_OUT',
  defaultValue: 'build/manual_shots',
);
final _only = const String.fromEnvironment('SHOT_ONLY');

/// Sign-in buttons stand-in: the real button row, fed the four providers the console can switch on.
/// (In a test the app has no native sign-in SDK keys, so the sign-in screen itself shows none.)
class _Provider extends SocialSignIn {
  const _Provider(this.provider);
  @override
  final String provider;
  @override
  Future<String?> obtainToken(BuildContext context) async => null;
}

/// The owner's colour from the business profile (logo needs an uploaded file, so it is not shown).
final _branded = brandFromProfile(
  const {'name': '말끔 청소', 'primary_color': '#1E6FD9'},
  fallback: BrandConfig.placeholder,
  apiBase: TestEnv.baseUrl,
);

final _shots = <(String, Widget Function(), BrandConfig)>[
  (
    'customer-login-social',
    () => Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(Space.gutter),
          child: SocialSignInRow(
            providers: const [
              _Provider('kakao'),
              _Provider('naver'),
              _Provider('google'),
              _Provider('apple'),
            ],
            onSelected: (_) {},
          ),
        ),
      ),
    ),
    BrandConfig.placeholder,
  ),
  (
    'customer-home',
    () => Scaffold(body: HomeTab(onOpenTab: (_) {})),
    BrandConfig.placeholder,
  ),
  (
    'customer-service',
    () => ServiceScreen(
      service: Service.fromJson(serviceJson('bathroom')),
      initialUnit: 1,
    ),
    BrandConfig.placeholder,
  ),
  (
    'customer-booking-flow',
    () => storeDraftScreen((d) => ScheduleScreen(draft: d)),
    BrandConfig.placeholder,
  ),
  (
    'customer-quote-flow',
    () => QuoteFlowScreen(service: Service.fromJson(serviceJson('move_in'))),
    BrandConfig.placeholder,
  ),
  (
    'customer-checkout',
    () => storeDraftScreen((d) => CheckoutScreen(draft: d)),
    BrandConfig.placeholder,
  ),
  (
    'customer-booking-detail',
    () => const BookingDetailScreen(id: 'bk-1'),
    BrandConfig.placeholder,
  ),
  (
    'customer-chat',
    () => const ChatRoomScreen(
      threadId: 'th-1',
      topic: 'bathroom',
      bookingId: 'bk-1',
    ),
    BrandConfig.placeholder,
  ),
  (
    'customer-more',
    () => const Scaffold(body: MoreTab()),
    BrandConfig.placeholder,
  ),
  (
    'customer-home-branded',
    () => Scaffold(body: HomeTab(onOpenTab: (_) {})),
    _branded,
  ),
];

void main() {
  for (final lang in const ['ko', 'en']) {
    testWidgets('manual shots (customer, $lang)', (tester) async {
      await tester.runAsync(loadShotFonts);
      final env = await storeShotEnv(lang, ThemeMode.light);
      for (final (id, screen, brand) in _shots) {
        if (_only.isNotEmpty && !_only.split(',').contains(id)) continue;
        final app = AppState(env.api, FakeRealtime(env.api));
        // The Shell loads these on start in the real app; screens shot alone need them up front.
        await tester.runAsync(app.loadAll);
        await captureShot(
          tester,
          CustomerTestHost(env: env, app: app, home: screen(), brand: brand),
          _phone,
          '$_out/$lang/$id.png',
        );
        await unmount(tester);
      }
    });
  }
}
