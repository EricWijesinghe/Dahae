// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images for the 2nd edition: rebranding, the domain move, store build, server settings,
// business hours, the Toss payouts switch and the PortOne slot clear (chapters 11, 13, 17, 28).
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'package:console/brand_assets.dart';
import 'package:console/host_job.dart';
import 'package:console/main.dart';
import 'package:console/screens/brand_preview.dart';
import 'package:console/screens/host_controls.dart';
import 'package:console/screens/setup.dart';
import 'package:console/screens/setup_business.dart';
import 'package:console/screens/setup_domain.dart';
import 'package:console/screens/setup_guides.dart';
import 'package:console/screens/setup_operator.dart';
import 'package:console/screens/setup_store_build.dart';
import 'package:console/state.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'manual_fixtures.dart';
import 'manual_support.dart';

// Sample addresses only; 203.0.113.0/24 and 198.51.100.0/24 are documentation ranges (the IP is masked).
const _old = 'old-example.co.kr';
const _new = 'example.co.kr';
const _ip = '203.0.113.10';
const _token = '0123456789abcdef0123456789abcdef';
const _color = Color(0xFF1E6FD9);

/// A short page for the brand dialogs, so the print is not mostly grey backdrop.
const _dialogView = ShotDevice('dialog', 1600, 1300, 2);

String _in(Duration d) => DateTime.now().add(d).toUtc().toIso8601String();

Json _named() => {
  ...sampleBusiness(),
  'name_en': 'Malkkeum Cleaning',
  'short_name': '말끔',
  'tagline': '집안일, 말끔하게 해드려요',
  'tagline_en': 'Home cleaning, done right',
};

Json _checklist() => {
  'current': _new,
  'aliases': [
    {'host': _old, 'since': '2026-11-20T00:00:00Z'},
  ],
  'checklist': [
    {
      'id': 'portone_webhook',
      'value': 'https://$_new/v1/webhooks/portone',
      'done': true,
    },
    {
      'id': 'toss_payments_webhook',
      'value': 'https://$_new/v1/webhooks/tosspayments',
      'done': true,
    },
    {
      'id': 'naver_callback',
      'value': 'https://$_new/auth/naver/callback',
      'done': false,
    },
    {'id': 'search_console', 'value': 'https://$_new', 'done': false},
    {
      'id': 'play_urls',
      'value': 'https://$_new/privacy.html\nhttps://$_new/delete-account.html',
      'done': false,
    },
    {
      'id': 'github_store_vars',
      'value': 'API_BASE=https://$_new\nAPP_LINK_DOMAIN=$_new,$_old',
      'done': false,
    },
  ],
};

/// The server side of a domain move (see test/domain_card_test.dart): DNS check, a held `domain-set`,
/// the approval, then the move ends ok and `/admin/domains` answers from the new address.
void _domainApi(FakeApi f, {bool match = true}) {
  var approved = false, polls = 0, moved = false;
  f
    ..on(
      'GET',
      '/admin/domains',
      (_) => moved
          ? _checklist()
          : {'current': _old, 'aliases': <Json>[], 'checklist': <Json>[]},
    )
    ..on(
      'POST',
      '/admin/host/jobs',
      (c) => ApiReply(202, {
        'id': (c.body as Json)['kind'] == 'domain-check' ? 'chk' : 'set',
        'progress_token': _token,
      }),
    )
    ..reply('GET', '/ops-progress/$_token', {
      'state': 'awaiting_approval',
      'summary': '도메인을 $_new 로 바꿔요 (예전 주소도 계속 응답해요)',
    })
    ..reply('GET', '/admin/host/jobs/chk', {'id': 'chk', 'state': 'ok'})
    ..reply('GET', '/admin/host/jobs/chk/output', {
      'text':
          '{"domain":"$_new","server_ip":"$_ip","a":["${match ? _ip : '198.51.100.7'}"],"aaaa":[],"match":$match,"aaaa_ok":true}',
    })
    ..on('GET', '/admin/host/jobs/set', (_) {
      final n = approved ? ++polls : 0;
      if (n <= 1) {
        return {
          'id': 'set',
          'state': 'awaiting_approval',
          'progress_token': _token,
        };
      }
      if (n == 2) return {'id': 'set', 'state': 'running'};
      moved = true;
      return {'id': 'set', 'state': 'ok', 'step': '완료'};
    })
    ..on('POST', '/admin/host/jobs/set/approve', (_) {
      approved = true;
      return null;
    })
    ..reply('PUT', '/admin/domains/checklist/naver_callback', null);
}

Widget _domainCard(TestEnv env) =>
    cardHost(env, DomainCard(business: sampleBusiness()));

Future<void> _code(WidgetTester tester, T t, String code) async {
  await tester.enterText(find.byType(TextField).last, code);
  await tester.tap(find.widgetWithText(FilledButton, t.oaContinue));
  await shotSettle(tester);
}

Future<void> _checkDns(WidgetTester tester, T t) async {
  await tapText(tester, t.cRbDomainChange);
  await tester.enterText(find.byType(TextField), _new);
  await tapText(tester, t.cRbDomainCheck);
  await shotSettle(tester);
}

/// DNS checked and the owner code given: the server approval code dialog is open.
Future<void> _toApproval(WidgetTester tester, T t) async {
  await _checkDns(tester, t);
  await tapText(tester, t.cRbDomainApply);
  await _code(tester, t, '111111');
  await shotSettle(tester);
}

ManualShot _domainShot(
  String id,
  Future<void> Function(WidgetTester tester, T t) act, {
  bool match = true,
  Json? data,
}) => ManualShot(
  id,
  device: cardView,
  build: _domainCard,
  api: data != null
      ? (f) => f.reply('GET', '/admin/domains', data)
      : (f) => _domainApi(f, match: match),
  act: act,
);

/// Lets the engine finish image work a tap started (letter mark), then shows it.
Future<void> _realWait(WidgetTester tester) async {
  for (var i = 0; i < 6; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump();
  }
  await shotSettle(tester);
}

Widget _business(TestEnv env, Json initial) =>
    cardHost(env, BusinessCard(initial: initial, onSaved: () {}));

BuildContext _ctx(WidgetTester tester) =>
    tester.element(find.byType(BusinessCard));

/// A saved PortOne card, unlocked, with channel keys in four slots.
Json _savedPortone() => setupCard(
  'portone',
  [
    field('store_id', 'text', required: true, set: true, preview: 'store-••••'),
    field('api_secret', 'secret', required: true, set: true, preview: '••••'),
    field('webhook_secret', 'secret', set: true, preview: '••••'),
    field(
      'channel_keys',
      'map',
      set: true,
      preview: {
        for (final k in ['card', 'card_billing', 'kakaopay', 'identity'])
          k: 'channel-key-••••',
      },
    ),
    field('cash_receipts', 'toggle'),
  ],
  configured: true,
  unlockUntil: _in(const Duration(minutes: 4)),
);

final _shots = <ManualShot>[
  // 11 PortOne: one slot cleared to move KakaoPay to Toss Payments
  ManualShot(
    'console-setup-portone-clear-slot',
    device: cardView,
    build: (env) => cardHost(
      env,
      IntegrationCard(
        card: _savedPortone(),
        onChanged: () {},
        webhookUrl: webhookUrl,
        tossWebhookUrl: tossPayoutWebhookUrl,
        tossPaymentsWebhookUrl: tossPaymentsWebhookUrl,
      ),
    ),
    act: (tester, t) async {
      await tester.tap(find.byType(ExpansionTile).first);
      await shotSettle(tester);
      final kakao = find.widgetWithText(
        TextField,
        slotLabels['kakaopay']!.of(t.localeName),
      );
      await scrollTo(tester, kakao, alignment: 0.3);
      await tester.tap(
        find.descendant(of: kakao, matching: find.byType(IconButton)),
      );
      await shotSettle(tester);
    },
  ),
  // 13 Partner payouts: the owner switch at the bottom of Setup
  ManualShot(
    'console-setup-payout-switch',
    device: cardView,
    build: (env) => cardHost(env, const TossPayoutSwitchCard(on: false)),
  ),
  // 17 Business hours
  ManualShot(
    'console-setup-business-hours',
    device: cardView,
    api: (f) => f.reply('GET', '/admin/setup/business-hours', {
      'weekdays': [0, 1, 2, 3, 4],
      'open': '09:00',
      'close': '18:00',
    }),
    build: (env) => cardHost(env, const BusinessHoursCard()),
  ),
  // 28 Rebranding
  ManualShot(
    'console-setup-business-names',
    device: cardView,
    build: (env) => _business(env, _named()),
    act: (tester, t) async {
      await tester.tap(find.byType(ExpansionTile).first);
      await shotSettle(tester);
      await tester.tap(find.text(t.cRbLetterMark));
      await _realWait(tester);
    },
  ),
  ManualShot(
    'console-setup-business-letter-logo',
    device: cardView,
    build: (env) => _business(env, _named()),
    act: (tester, t) async {
      await tester.tap(find.byType(ExpansionTile).first);
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-brand-preview',
    device: _dialogView,
    build: (env) => _business(env, _named()),
    act: (tester, t) async {
      final files = await tester.runAsync(
        () async => deriveBrandAssets(
          logo: await decodeLogo(await letterMark('말끔 청소', _color)),
          color: _color,
          name: '말끔 청소',
          tagline: '집안일, 말끔하게 해드려요',
        ),
      );
      confirmBrandPreview(_ctx(tester), files!).ignore();
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-brand-applied',
    device: _dialogView,
    build: (env) => _business(env, _named()),
    act: (tester, t) async {
      showBrandApplied(_ctx(tester)).ignore();
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-setup-store-build',
    device: cardView,
    build: (env) => cardHost(
      env,
      StoreBuildCard(
        data: const {
          'changed': ['name', 'icon'],
          'acked_at': null,
          'build_url': 'https://github.com/<owner>/<repo>/actions',
        },
        onAcked: () {},
      ),
    ),
  ),
  ManualShot(
    'console-ops-approval-enrol',
    device: cardView,
    api: (f) => f
      ..reply('POST', '/admin/host/jobs', {'id': 'en'}, status: 202)
      ..reply('GET', '/admin/host/jobs/en', {'id': 'en', 'state': 'ok'})
      ..reply('GET', '/admin/host/jobs/en/output', {
        // Not a real key: the test font draws the X strings as solid bars, which works as the blur.
        'text':
            'otpauth://totp/Dahae:server?secret=XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX&issuer=Dahae\n',
      }),
    build: (env) => consoleScreen(
      env,
      ListView(
        padding: const EdgeInsets.all(24),
        children: const [
          ApprovalEnrollCard(),
          SizedBox(height: 32),
          ApprovalEnrollCard(),
        ],
      ),
    ),
    act: (tester, t) async {
      await tester.tap(find.text(t.cRbEnrollStart).last);
      await shotSettle(tester);
      await _code(tester, t, '123456');
      await shotSettle(tester, duration: const Duration(seconds: 6));
    },
  ),
  _domainShot('console-setup-domain-dns-ok', _checkDns),
  _domainShot('console-setup-domain-dns-bad', _checkDns, match: false),
  _domainShot('console-domain-approval-code', _toApproval),
  _domainShot('console-setup-domain-done', (tester, t) async {
    await _toApproval(tester, t);
    await _code(tester, t, '222222');
    await shotSettle(tester, duration: const Duration(seconds: 8));
  }),
  _domainShot(
    'console-setup-domain-checklist',
    (tester, t) =>
        scrollTo(tester, find.text(t.cRbChecklistTitle), alignment: 0.02),
    data: _checklist(),
  ),
  _domainShot('console-domain-retire-dialog', (tester, t) async {
    await tapText(tester, t.cRbDomainRetire);
    await tester.tap(find.byType(CheckboxListTile));
    await shotSettle(tester);
  }, data: _checklist()),
  ManualShot(
    'console-server-moved-banner',
    api: (f) => f.reply('GET', '/config', {'api_base': 'https://$_new'}),
    build: (env) => ConsoleApp(
      session: env.session,
      state: ConsoleState(env.api, FakeRealtime(env.api), env.session),
      locale: env.locale,
      theme: env.theme,
      onChangeServer: () {},
    ),
  ),
];

void main() {
  setUp(() => hostJobPoll = const Duration(milliseconds: 100));
  runManualShots('rebrand', _shots, addBase);
}
