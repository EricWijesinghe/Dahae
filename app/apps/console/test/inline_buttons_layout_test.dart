// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// The shared theme gives FilledButton a full-width minimum. Placed in a ListTile slot or a Row it
// squeezed the titles beside it to one letter per line (the 내 계정 screen) or failed to lay out;
// these screens must render clean, with titles on one line or wrapping by words, at phone and desktop width.

import 'dart:math' as math;

import 'package:console/screens/confirm_action.dart';
import 'package:console/screens/finance_tax.dart';
import 'package:console/screens/member_approvals.dart';
import 'package:console/screens/owner_account.dart';
import 'package:console/screens/people.dart';
import 'package:console/screens/privacy_requests.dart';
import 'package:console/screens/setup_pay_methods.dart';
import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'manual_screenshots/manual_fixtures.dart';
import 'support/console_host.dart';

const _sizes = [Size(360, 800), Size(1280, 800)];

Json _ownerMe() => {
  'is_owner': true,
  'owner_exists': true,
  'two_factor_enrolled': true,
  'recovery_confirmed': true,
  'alert_phone_masked': '010-****-5678',
  'active_sessions': 2,
};

/// One screen to check: what it loads, how to reach the state under test, and which texts must not be squeezed.
class LayoutCase {
  const LayoutCase(
    this.name, {
    required this.home,
    required this.titles,
    this.api,
    this.act,
  });
  final String name;
  final Widget home;
  final void Function(FakeApi f)? api;
  final Future<void> Function(WidgetTester tester, T t)? act;
  final List<String> Function(T t) titles;
}

final layoutCases = [
  LayoutCase(
    'owner account (owner)',
    home: const OwnerAccountScreen(),
    api: (f) => f.reply('GET', '/admin/me/account', _ownerMe()),
    titles: (t) => [
      t.oaAlertPhone,
      t.oaPassword,
      t.oaPasswordNote,
      t.oaRecoveryCodes,
      t.oaSignOutAll,
      t.oaTransfer,
      t.oaTransferBody,
    ],
  ),
  LayoutCase(
    'owner account (admin, claim)',
    home: const OwnerAccountScreen(),
    api: (f) => f.reply('GET', '/admin/me/account', {
      ..._ownerMe(),
      'is_owner': false,
      'owner_exists': false,
    }),
    titles: (t) => [t.oaClaimTitle, t.oaClaimBody],
  ),
  LayoutCase(
    'member approvals',
    home: const MemberApprovals(),
    api: (f) => f.reply('GET', '/admin/team-members/pending', [
      {'id': 'm1', 'team_name': '말끔팀', 'name': '김하나', 'phone': '01012345678'},
    ]),
    titles: (t) => ['말끔팀 · 김하나', '${t.cPro_pending} · 01012345678'],
  ),
  LayoutCase(
    'tax invoice queue and VAT export',
    home: ListView(
      padding: const EdgeInsets.all(16),
      children: [
        TaxSection(
          periodQuery: const {},
          onExport: (_, _, _) async {},
          onReExport: () async {},
        ),
      ],
    ),
    api: (f) => f
      ..reply('GET', '/admin/finance/tax-invoices', [
        {
          'id': 'ti1',
          'state': 'requested',
          'company_name': '해맑음 주식회사',
          'business_number': '123-45-67890',
          'approval_number': null,
        },
      ])
      ..reply('GET', '/admin/finance/cash-receipts', const <Json>[]),
    titles: (t) => ['해맑음 주식회사 · 123-45-67890', t.tCashPending],
  ),
  LayoutCase(
    'partners list (unverified pro)',
    home: const PartnersScreen(),
    api: (f) => f
      ..reply('GET', '/admin/team-members/pending', const <Json>[])
      ..reply('GET', '/admin/pros', [
        {
          'id': 'p1',
          'team_name': '말끔팀',
          'name': '김하나',
          'phone': '01012345678',
          'status': 'pending',
          'completed': 3,
          'cancelled': 0,
          'rating': 4.8,
          'fee_bps': 1500,
          'quality_score': null,
          'quality_status': null,
        },
      ]),
    titles: (_) => ['말끔팀 · 김하나'],
  ),
  LayoutCase(
    'privacy requests search and match',
    home: const PrivacyRequestsScreen(),
    api: (f) => f
      ..reply('GET', '/admin/privacy/requests', const <Json>[])
      ..reply('GET', '/admin/privacy/customers', [
        {'id': 'c1', 'name': '이지민', 'phone': '01012345678', 'bookings': 2},
      ]),
    act: (tester, t) async {
      await tester.enterText(find.byType(TextField).first, '01012345678');
      await tester.tap(find.text(t.prqFind));
      await tester.pumpAndSettle();
    },
    titles: (t) => [t.prqExport, t.prqDelete],
  ),
  LayoutCase(
    'payment methods card',
    home: ListView(children: const [PayMethodsCard()]),
    api: (f) => f.reply('GET', '/admin/payment-methods', payMethods()),
    act: (tester, t) async {
      await tester.tap(find.text(t.cSetupPayMethods));
      await tester.pumpAndSettle();
    },
    titles: (t) => [t.payMethod('card'), t.payMethod('virtual_account')],
  ),
];

/// A title is squeezed when it is narrower than it would be on one line and also under four letters wide.
void expectNotSqueezed(WidgetTester tester, String text) {
  final found = find.text(text);
  expect(found, findsWidgets, reason: '"$text" is on screen');
  for (final e in found.evaluate()) {
    final p = e.renderObject! as RenderParagraph;
    final fontSize = p.text.style?.fontSize ?? 14;
    final oneLine = p.getMaxIntrinsicWidth(double.infinity);
    expect(
      p.size.width,
      greaterThanOrEqualTo(math.min(oneLine, 4 * fontSize) - 0.5),
      reason: '"$text" is squeezed to ${p.size.width}px',
    );
  }
}

Future<void> setViewSize(WidgetTester tester, Size size) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

void main() {
  for (final c in layoutCases) {
    for (final lang in const ['ko', 'en']) {
      for (final size in _sizes) {
        testWidgets('${c.name}: lays out at ${size.width.toInt()} wide ($lang)', (
          tester,
        ) async {
          final fake = FakeApi();
          c.api?.call(fake);
          final env = await TestEnv.create(
            fake: fake,
            localeCode: lang,
            themeMode: ThemeMode.light,
            role: 'staff_admin',
          );
          await setViewSize(tester, size);
          await pumpConsoleScreen(tester, env, c.home);
          await tester.pumpAndSettle();
          final t = lookupT(Locale(lang));
          await c.act?.call(tester, t);
          expect(tester.takeException(), isNull);
          for (final title in c.titles(t)) {
            // A phone-height list builds its lower cards only once they scroll near.
            for (
              var i = 0;
              i < 20 && find.text(title).evaluate().isEmpty;
              i++
            ) {
              await tester.drag(
                find.byType(Scrollable).first,
                const Offset(0, -200),
              );
              await tester.pumpAndSettle();
            }
            expectNotSqueezed(tester, title);
          }
        });
      }
    }
  }

  for (final size in _sizes) {
    testWidgets(
      'confirm dialog keeps Cancel and OK on one row at ${size.width.toInt()}',
      (tester) async {
        final env = await TestEnv.create(
          localeCode: 'ko',
          themeMode: ThemeMode.light,
          role: 'staff_admin',
        );
        await setViewSize(tester, size);
        await pumpConsoleScreen(
          tester,
          env,
          Builder(
            builder: (context) => TextButton(
              onPressed: () => confirmAction(context, '삭제'),
              child: const Text('open'),
            ),
          ),
        );
        await tester.tap(find.text('open'));
        await tester.pumpAndSettle();
        expect(tester.takeException(), isNull);
        final dialog = find.byType(AlertDialog);
        final cancel = find.descendant(
          of: dialog,
          matching: find.byType(TextButton),
        );
        final ok = find.descendant(
          of: dialog,
          matching: find.byType(FilledButton),
        );
        expect(
          tester.getCenter(cancel).dy,
          moreOrLessEquals(tester.getCenter(ok).dy, epsilon: 1),
        );
      },
    );
  }
}
