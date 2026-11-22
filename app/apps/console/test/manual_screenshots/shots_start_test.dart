// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images, chapters 06 to 17: first sign-in, the console frame and the setup page.
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'package:console/screens/first_run.dart';
import 'package:console/screens/host_controls.dart';
import 'package:console/screens/setup.dart';
import 'package:console/screens/setup_app_links.dart';
import 'package:console/screens/setup_business.dart';
import 'package:console/screens/setup_guides.dart';
import 'package:console/screens/setup_pay_methods.dart';
import 'package:console/screens/setup_review.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'manual_fixtures.dart';
import 'manual_support.dart';

const _tokens = {
  'access_token': 'a-1',
  'refresh_token': 'r-1',
  'role': 'staff_admin',
  'subject': 'staff-1',
};

/// Signs in on the real sign-in screen with a sample owner email.
Future<void> _signIn(WidgetTester tester, T t) async {
  await tester.enterText(find.byType(TextField).at(0), 'owner@example.co.kr');
  await tester.enterText(find.byType(TextField).at(1), 'sample-password-1');
  await tester.tap(find.widgetWithText(PrimaryButton, t.cLogin));
  await shotSettle(tester);
}

void _enrolApi(FakeApi f) => f
  ..reply('POST', '/auth/staff/login', {
    'mfa_enrol_required': true,
    'mfa_token': 'mfa-1',
  })
  ..reply('POST', '/auth/staff/mfa/enrol', {
    // Not a real key: the manual shows where the key appears, not a usable one.
    'secret': 'XXXXXXXXXXXXXXXX',
    'otpauth_uri': 'otpauth://totp/Dahae:owner%40example.co.kr?secret=XXXX',
  })
  ..reply('POST', '/auth/staff/mfa/enrol/confirm', {
    ..._tokens,
    'recovery_codes': [for (var i = 0; i < 8; i++) 'XXXX-XXXX-XXXX-XXXX'],
  });

/// One integration card on its own, the way the setup page lists it.
Widget _card(TestEnv env, Json card) => cardHost(
  env,
  IntegrationCard(
    card: card,
    onChanged: () {},
    webhookUrl: webhookUrl,
    tossWebhookUrl: tossPayoutWebhookUrl,
    tossPaymentsWebhookUrl: tossPaymentsWebhookUrl,
  ),
);

ManualShot _cardShot(
  String id,
  Json Function() card, {
  Future<void> Function(WidgetTester tester, T t)? act,
}) => ManualShot(
  id,
  device: cardView,
  build: (env) => _card(env, card()),
  act: act,
);

Future<void> _expand(WidgetTester tester, T t) async {
  await tester.tap(find.byType(ExpansionTile).first);
  await shotSettle(tester);
}

Json _fresh(String code) => setupCard(code, cardFields[code]!);

Json _savedKakao({Json? lastTest}) => setupCard(
  'kakao',
  [
    field('app_id', 'text', required: true, set: true, preview: '1234567'),
    field('admin_key', 'secret', set: true, preview: '••••'),
  ],
  configured: true,
  lastTest: lastTest,
);

/// A saved payout card: any saved card is locked until the owner unlocks it.
Json _lockedPayout() => setupCard(
  'payout',
  [
    field('mode', 'text', required: true, set: true, preview: 'toss'),
    field('secret_key', 'secret', set: true, preview: '••••'),
    field('security_key', 'secret', set: true, preview: '••••'),
  ],
  configured: true,
  locked: true,
);

String _in(DateTime d) => d.toUtc().toIso8601String();

final _shots = <ManualShot>[
  // 06 First sign-in
  ManualShot(
    'console-first-run',
    build: (env) => consoleScreen(env, const FirstRunScreen()),
  ),
  const ManualShot('console-login', build: consoleApp, signedIn: false),
  const ManualShot(
    'console-mfa-enrol',
    build: consoleApp,
    signedIn: false,
    api: _enrolApi,
    act: _signIn,
  ),
  ManualShot(
    'console-mfa-recovery-codes',
    build: consoleApp,
    signedIn: false,
    api: _enrolApi,
    act: (tester, t) async {
      await _signIn(tester, t);
      await tester.enterText(find.byType(TextField).first, '123456');
      await tester.tap(find.widgetWithText(PrimaryButton, t.cMfaConfirm));
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-mfa-signin',
    build: consoleApp,
    signedIn: false,
    api: (f) => f.reply('POST', '/auth/staff/login', {
      'mfa_required': true,
      'mfa_token': 'mfa-1',
    }),
    act: _signIn,
  ),
  // 08 The console frame
  const ManualShot('console-menu-desktop', build: consoleApp),
  ManualShot(
    'console-menu-phone',
    build: consoleApp,
    device: consolePhone,
    act: (tester, t) async {
      tester
          .state<ScaffoldState>(
            find.byWidgetPredicate((w) => w is Scaffold && w.drawer != null),
          )
          .openDrawer();
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-palette',
    build: consoleApp,
    act: (tester, t) async {
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await shotSettle(tester);
      await tester.enterText(
        find.byWidgetPredicate(
          (w) => w is TextField && w.decoration?.hintText == t.cPaletteHint,
        ),
        t.cSetup.characters.take(2).toString(),
      );
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-my-account',
    build: consoleApp,
    api: (f) => f.reply('GET', '/admin/me/account', {
      'is_owner': true,
      'owner_exists': true,
      'two_factor_enrolled': true,
      'recovery_confirmed': true,
      'alert_phone_masked': '010-****-5678',
      'active_sessions': 2,
    }),
    act: (tester, t) => openSection(tester, t.oaTitle),
  ),
  // 09 The setup page
  ManualShot(
    'console-setup-overview',
    build: consoleApp,
    act: (tester, t) => openSection(tester, t.cSetup),
  ),
  _cardShot('console-setup-card-guide-open', () => _fresh('solapi')),
  _cardShot('console-setup-card-locked', _lockedPayout, act: _expand),
  _cardShot(
    'console-setup-owner-code',
    _lockedPayout,
    act: (tester, t) async {
      await _expand(tester, t);
      await scrollTo(tester, find.text(t.cSetupUnlock), alignment: 0.5);
      await tester.tap(find.text(t.cSetupUnlock));
      await shotSettle(tester);
    },
  ),
  _cardShot(
    'console-setup-test-ok',
    () => _savedKakao(lastTest: {'ok': true, 'code': 'connected'}),
    act: _expand,
  ),
  _cardShot(
    'console-setup-test-invalid',
    () => _savedKakao(lastTest: {'ok': false, 'code': 'invalid_key'}),
    act: _expand,
  ),
  // 10 Business profile and payment methods
  ManualShot(
    'console-setup-business',
    device: cardView,
    build: (env) =>
        cardHost(env, BusinessCard(initial: sampleBusiness(), onSaved: () {})),
    act: _expand,
  ),
  ManualShot(
    'console-setup-business-color-warning',
    device: cardView,
    build: (env) => cardHost(
      env,
      BusinessCard(
        initial: sampleBusiness(color: '#FFE066'),
        onSaved: () {},
      ),
    ),
    act: (tester, t) async {
      await _expand(tester, t);
      await scrollTo(tester, find.text(t.cSetupColor), alignment: 0.3);
    },
  ),
  ManualShot(
    'console-setup-store-links',
    device: cardView,
    build: (env) =>
        cardHost(env, BusinessCard(initial: sampleBusiness(), onSaved: () {})),
    act: (tester, t) async {
      await _expand(tester, t);
      await scrollTo(tester, find.text(t.cSetupStoreLinks));
    },
  ),
  ManualShot(
    'console-setup-app-versions',
    device: cardView,
    build: (env) =>
        cardHost(env, BusinessCard(initial: sampleBusiness(), onSaved: () {})),
    act: (tester, t) async {
      await _expand(tester, t);
      await scrollTo(tester, find.text(t.uAppVersionTitle));
    },
  ),
  ManualShot(
    'console-setup-pay-methods',
    device: cardView,
    build: (env) => cardHost(env, const PayMethodsCard()),
    act: _expand,
  ),
  // 11 PortOne
  _cardShot('console-setup-portone', () => _fresh('portone')),
  _cardShot(
    'console-setup-portone-channels',
    () => _fresh('portone'),
    act: (tester, t) => scrollTo(
      tester,
      find.text(slotLabels['card']!.of(t.localeName)),
      alignment: 0.15,
    ),
  ),
  _cardShot(
    'console-setup-portone-webhook',
    () => _fresh('portone'),
    act: (tester, t) =>
        scrollTo(tester, find.text(t.cSetupWebhook), alignment: 0.2),
  ),
  // 12 Toss Payments
  _cardShot('console-setup-toss', () => _fresh('tosspayments')),
  _cardShot(
    'console-setup-toss-active',
    () => setupCard(
      'tosspayments',
      savedTossFields(active: true),
      configured: true,
      unlockUntil: _in(DateTime.now().add(const Duration(minutes: 9))),
      mode: 'test',
      lastTest: {'ok': true, 'code': 'sandbox_connected'},
    ),
    act: (tester, t) async {
      await _expand(tester, t);
      await scrollTo(tester, find.byType(SwitchListTile), alignment: 0.6);
    },
  ),
  ManualShot(
    'console-payments-header-test-keys',
    build: consoleApp,
    api: (f) => f.reply('GET', '/admin/setup', {
      ...freshSetup(),
      'payment_gateways': [
        {'code': 'portone', 'mode': null},
        {'code': 'tosspayments', 'mode': 'test'},
      ],
      'payment_gateway': 'portone',
      'payment_gateway_mode': null,
    }),
    act: (tester, t) async {
      await openSection(tester, t.cSetup);
      await scrollTo(tester, find.text(t.cPaymentsTitle), within: SetupScreen);
    },
  ),
  // 13 Partner payouts
  _cardShot('console-setup-payout', () => _fresh('payout')),
  // 14 Text messages
  _cardShot('console-setup-solapi', () => _fresh('solapi')),
  _cardShot(
    'console-setup-solapi-templates',
    () => _fresh('solapi'),
    act: (tester, t) =>
        scrollTo(tester, find.text('booking.status'), alignment: 0.3),
  ),
  // 15 Social sign-in and app links
  _cardShot('console-setup-kakao', () => _fresh('kakao')),
  _cardShot('console-setup-naver', () => _fresh('naver')),
  _cardShot('console-setup-google', () => _fresh('google')),
  _cardShot('console-setup-apple', () => _fresh('apple')),
  ManualShot(
    'console-setup-app-links',
    device: cardView,
    build: (env) => cardHost(env, const AppLinksCard()),
  ),
  // 16 Push, AI, address search
  _cardShot('console-setup-push', () => _fresh('push')),
  _cardShot('console-setup-ai', () => _fresh('ai')),
  _cardShot('console-setup-juso', () => _fresh('juso')),
  // 17 Backup, uptime, store review account
  _cardShot(
    'console-setup-backup',
    () => _fresh('backup'),
    act: (tester, t) async {
      final boxes = find.byType(TextField);
      const values = [
        'https://<account-id>.r2.cloudflarestorage.com',
        'auto',
        'dahae-backup',
        'sample-access-key',
        'sample-secret-key',
        '30',
      ];
      for (var i = 0; i < values.length; i++) {
        await tester.enterText(boxes.at(i), values[i]);
      }
      await shotSettle(tester);
      await scrollTo(tester, boxes.first, alignment: 0.1);
    },
  ),
  ManualShot(
    'console-setup-heartbeat',
    device: cardView,
    build: (env) => consoleScreen(
      env,
      ListView(
        padding: const EdgeInsets.all(24),
        children: const [
          HeartbeatCard(configured: false),
          SizedBox(height: 16),
          HeartbeatCard(configured: true),
        ],
      ),
    ),
  ),
  ManualShot(
    'console-setup-review-account',
    device: cardView,
    build: (env) => cardHost(
      env,
      ReviewAccountCard(
        current: {
          'phone': '01000000000',
          'expires_at': _in(DateTime.now().add(const Duration(days: 14))),
        },
        onChanged: () {},
      ),
    ),
    act: _expand,
  ),
];

void main() => runManualShots('start', _shots, addBase);
