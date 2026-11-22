// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

// Manual book images, chapters 18 to 23: prices, daily work, people, money, marketing, rules.
// Excluded from normal runs (tag `manual_screenshots`); see scripts/manual/shots.sh.
@Tags(['manual_screenshots'])
library;

import 'dart:async';

import 'package:console/screens/catalog_counts.dart';
import 'package:console/screens/cases.dart';
import 'package:console/screens/finance.dart';
import 'package:console/screens/growth.dart';
import 'package:console/screens/people_profile.dart';
import 'package:console/screens/points_ledger.dart';
import 'package:console/screens/review_reports.dart';
import 'package:console/screens/staff_invite.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';

import 'daily_fixtures.dart';
import 'manual_fixtures.dart';
import 'manual_support.dart';

/// Every read the daily screens make, with sample rows.
void _daily(FakeApi f) {
  f
    ..reply('GET', '/admin/services', adminServices())
    ..reply('GET', '/admin/service-groups', serviceGroups())
    ..reply('GET', '/admin/slot-capacity', [
      for (final h in [9, 10, 13, 14, 15]) [h, h == 9 ? 4 : null],
    ])
    ..reply('GET', '/admin/quote-formula', {
      'base_studio': 150000,
      'base_one_and_half': 200000,
      'base_two_rooms': 260000,
      'base_three_rooms': 330000,
      'base_four_plus': 400000,
      'per_extra_bathroom': 30000,
      'per_balcony': 20000,
      'duplex': 50000,
      'no_elevator_high_floor': 30000,
      'ac_addon_discount_bps': 1000,
      'type_bps': 1000,
      'luggage_bps': 1500,
      'vat_bps': 1000,
      'deposit_bps': 3000,
    })
    ..reply('GET', '/admin/peak-pricing', {
      'weekend_bps': 1000,
      'public_holiday_bps': 1500,
      'lunar_period_bps': 2000,
      'evening_bps': 1000,
      'max_bps': 3000,
      'evening_start_hour': 18,
      'evening_end_hour': 22,
      'apply_to_fixed': false,
    })
    ..reply('GET', '/admin/quality-thresholds', {
      'warning_below': 80,
      'pause_below': 60,
    })
    ..reply('GET', '/policy/cancellation', {
      'is_default': true,
      'policy': {
        'tiers': [
          {'min_days': 3, 'percent': 0},
          {'min_days': 1, 'percent': 30},
          {'min_days': 0, 'percent': 50},
        ],
        'min_fee': 10000,
        'reschedule_min_days': 2,
      },
    })
    ..reply('GET', '/admin/bookings/bk-1', detailOf(confirmedBooking()))
    ..reply(
      'GET',
      '/admin/bookings/bk-q',
      detailOf(quoteBooking(), estimate: quoteEstimate),
    )
    ..reply('GET', '/admin/bookings/bk-.*/quotes', const <Json>[])
    ..reply('GET', '/admin/bookings', bookingRows())
    ..reply('GET', '/admin/threads', threads())
    ..reply('POST', '/admin/threads/.*/read', null)
    ..reply('GET', '/admin/ai/usage', {
      'configured': true,
      'mode': 'auto',
      'month': {'calls': 42},
    })
    ..reply('GET', '/admin/subscriptions', [
      subscription('sub-1', 'bathroom', 1, 'active'),
      subscription('sub-2', 'kitchen', 3, 'active'),
      subscription('sub-3', 'studio', 5, 'paused'),
    ])
    ..reply('GET', '/admin/pros/pro-3/profile', proProfile())
    ..reply('GET', '/admin/pros', pros())
    ..reply('GET', '/admin/customers', customers())
    ..reply('GET', '/admin/staff', staff())
    ..reply('GET', '/admin/growth/points/u-0', {
      'enabled': true,
      'balance': 2500,
      'entries': [
        {
          'kind': 'grant',
          'amount': 3000,
          'at': '2026-11-01T03:00:00Z',
          'reason': '서비스 보상',
        },
        {'kind': 'spend', 'amount': -500, 'at': '2026-11-12T03:00:00Z'},
      ],
    })
    ..reply('GET', '/admin/settlements/payouts', payouts())
    ..reply('GET', '/admin/settlements', settlements())
    ..reply('GET', '/admin/finance/summary', financeSummary())
    ..reply('GET', '/admin/finance/series', financeSeries())
    ..reply('GET', '/admin/finance/partners', financePartners())
    ..reply('GET', '/admin/finance/services', financeServices())
    ..reply('GET', '/admin/finance/tax-invoices', const <Json>[])
    ..reply('GET', '/admin/finance/cash-receipts', const <Json>[])
    ..reply('GET', '/admin/growth', {
      'welcome': {'enabled': true, 'kind': 'amount', 'value': 5000},
      'welcome_pack': [
        {'amount': 15000, 'service_code': null},
      ],
      'referral': {'enabled': true, 'amount': 5000},
    })
    ..reply('GET', '/admin/reviews/reports', const <Json>[])
    ..reply('GET', '/admin/reviews', const <Json>[])
    ..reply('GET', '/admin/cases', const <Json>[])
    ..reply('GET', '/admin/acquisition', {'rows': const <Json>[]})
    ..reply('GET', '/admin/campaigns', const <Json>[])
    ..reply('POST', '/admin/campaigns/preview', {
      'total': 412,
      'consented': 268,
    })
    ..reply('GET', '/admin/analytics/funnel', {
      'installs': 1240,
      'signed_up': 610,
      'quoted': 180,
      'booked': 142,
      'paid': 131,
      'repeat_buyers': 38,
    })
    ..reply('GET', '/admin/analytics/cohorts', [
      for (var w = 0; w < 4; w++)
        {
          'week': '2026-10-${(5 + w * 7).toString().padLeft(2, '0')}T00:00:00Z',
          'size': 40 + w * 6,
          'w1': 12,
          'w2': 9,
          'w3': 7,
          'w4': 6,
        },
    ])
    ..reply('GET', '/admin/settings/business', businessRules())
    ..reply('GET', '/admin/notify-templates', notifyTemplates())
    ..reply('GET', '/admin/audit', auditRows())
    ..reply('GET', '/admin/renewals', renewals());
  addBase(f);
}

/// Opens a section, then one of its tabs.
Future<void> _sectionTab(
  WidgetTester tester,
  String section,
  String tab,
) async {
  await openSection(tester, section);
  await tester.tap(find.widgetWithText(Tab, tab).first);
  await shotSettle(tester);
}

/// Runs a dialog function from inside the page, the way a button would.
Future<void> _dialog(
  WidgetTester tester,
  Future<Object?> Function(BuildContext context) open,
) async {
  unawaited(open(tester.element(find.byType(Scaffold).last)));
  await shotSettle(tester);
}

ManualShot _page(
  String id,
  String Function(T t) section, {
  Future<void> Function(WidgetTester tester, T t)? then,
  void Function(FakeApi f)? api,
}) => ManualShot(
  id,
  build: consoleApp,
  api: api,
  act: (tester, t) async {
    await openSection(tester, section(t));
    if (then != null) await then(tester, t);
  },
);

ManualShot _tab(
  String id,
  String Function(T t) section,
  String Function(T t) tab, {
  Future<void> Function(WidgetTester tester, T t)? then,
}) => ManualShot(
  id,
  build: consoleApp,
  act: (tester, t) async {
    await _sectionTab(tester, section(t), tab(t));
    if (then != null) await then(tester, t);
  },
);

Future<void> _tapRow(WidgetTester tester, String text) async {
  await tester.tap(find.textContaining(text).first);
  await shotSettle(tester);
}

final _shots = <ManualShot>[
  // 18 Catalog and prices
  _tab('console-catalog-pricing', (t) => t.cCatalog, (t) => t.cPricing),
  _tab(
    'console-catalog-new-service',
    (t) => t.cCatalog,
    (t) => t.cPricing,
    then: (tester, t) async {
      await tester.tap(find.text(t.cgNewService));
      await shotSettle(tester);
    },
  ),
  _tab(
    'console-catalog-groups',
    (t) => t.cCatalog,
    (t) => t.cgTabGroups,
    then: (tester, t) => _tapRow(tester, '이사·입주 청소'),
  ),
  _tab('console-catalog-capacity', (t) => t.cCatalog, (t) => t.cSlotCapacity),
  _tab('console-catalog-formula', (t) => t.cCatalog, (t) => t.cFormula),
  _tab('console-catalog-peak', (t) => t.cCatalog, (t) => t.cgTabPeak),
  _tab('console-catalog-quality', (t) => t.cCatalog, (t) => t.cgTabQuality),
  _tab(
    'console-catalog-cancel-policy',
    (t) => t.cCatalog,
    (t) => t.cTabCancelPolicy,
  ),
  ManualShot(
    'console-catalog-counts',
    build: (env) => consoleScreen(env, const SizedBox.expand()),
    act: (tester, t) =>
        _dialog(tester, (c) => askCountsPricing(c, airconCounts())),
  ),
  // 19 Daily work
  const ManualShot('console-dashboard', build: consoleApp),
  _page('console-bookings-list', (t) => t.cBookings),
  _page(
    'console-booking-detail',
    (t) => t.cBookings,
    then: (tester, t) => _tapRow(tester, 'B-20261121-0042'),
  ),
  _page(
    'console-booking-refund',
    (t) => t.cBookings,
    then: (tester, t) async {
      await _tapRow(tester, 'B-20261121-0042');
      await tester.tap(find.text(t.cCancelRefund).first);
      await shotSettle(tester);
    },
  ),
  _page(
    'console-quote-desk',
    (t) => t.cQuoteDesk,
    then: (tester, t) => _tapRow(tester, 'B-20261121-0031'),
  ),
  _page(
    'console-inbox',
    (t) => t.cInbox,
    api: (f) => f.reply('GET', '/admin/threads/th-1/messages', chatMessages()),
    then: (tester, t) => _tapRow(tester, t.service('bathroom')),
  ),
  _page(
    'console-inbox-ai',
    (t) => t.cInbox,
    api: (f) =>
        f.reply('GET', '/admin/threads/th-1/messages', chatMessages(ai: true)),
    then: (tester, t) => _tapRow(tester, t.service('bathroom')),
  ),
  _page(
    'console-subscriptions',
    (t) => t.cSubsTitle,
    then: (tester, t) async {
      await tester.tap(find.text(t.cgSubEdit).first);
      await shotSettle(tester);
    },
  ),
  // 20 People
  _page(
    'console-partners-list',
    (t) => t.cPartners,
    api: (f) => f.reply('GET', '/admin/team-members/pending', const <Json>[]),
  ),
  ManualShot(
    'console-partner-profile-checklist',
    build: (env) => consoleScreen(env, const SizedBox.expand()),
    act: (tester, t) =>
        _dialog(tester, (c) => showProProfile(c, 'pro-3', () async {})),
  ),
  _page('console-customers', (t) => t.cCustomers),
  ManualShot(
    'console-points-ledger',
    build: (env) =>
        consoleScreen(env, const PointsLedgerPage(userId: 'u-0', name: '김지민')),
    act: (tester, t) async {
      await tester.tap(find.byIcon(Icons.tune_rounded));
      await shotSettle(tester);
    },
  ),
  _page('console-staff-list', (t) => t.cStaff),
  _page(
    'console-staff-invite',
    (t) => t.cStaff,
    then: (tester, t) => _dialog(tester, (c) => inviteStaff(c, () async {})),
  ),
  // 21 Money
  _page('console-settlements', (t) => t.cSettlements),
  _page(
    'console-payout-account-reveal',
    (t) => t.cSettlements,
    api: (f) => f.reply('GET', '/admin/settlements/payouts/po-1/account', {
      'holder': '김민수',
      'bank': '국민은행',
      'account': '000000-00-••••••',
      'amount': 386000,
    }),
    then: (tester, t) async {
      await tester.tap(find.text(t.poTabPayouts).first);
      await shotSettle(tester);
      await tester.tap(find.text(t.poShowAccount).first);
      await shotSettle(tester);
    },
  ),
  _page(
    'console-stepup-prompt',
    (t) => t.cSettlements,
    api: (f) => f.reply('GET', '/admin/settlements/payouts/po-1/account', {
      'error': {'code': 'stepup_required'},
    }, status: 403),
    then: (tester, t) async {
      await tester.tap(find.text(t.poTabPayouts).first);
      await shotSettle(tester);
      await tester.tap(find.text(t.poShowAccount).first);
      await shotSettle(tester);
    },
  ),
  _page(
    'console-payouts-toss',
    (t) => t.cSettlements,
    api: (f) =>
        f.reply('GET', '/admin/settlements/payouts', payouts(toss: true)),
    then: (tester, t) async {
      await tester.tap(find.text(t.poTabPayouts).first);
      await shotSettle(tester);
    },
  ),
  _page('console-finance-overview', (t) => t.fFinance),
  _page(
    'console-finance-exports',
    (t) => t.fFinance,
    then: (tester, t) =>
        scrollTo(tester, find.text(t.fExportTitle), within: FinanceScreen),
  ),
  _page(
    'console-finance-tax',
    (t) => t.fFinance,
    then: (tester, t) =>
        scrollTo(tester, find.text(t.tTaxSection), within: FinanceScreen),
  ),
  // 22 Marketing and customer care
  _tab(
    'console-coupons',
    (t) => t.cMarketing,
    (t) => t.cIssueCoupon,
    then: (tester, t) async {
      await tester.enterText(
        find.widgetWithText(TextField, t.cCouponName),
        '첫 청소 5천원 할인',
      );
      await tester.enterText(find.widgetWithText(TextField, t.cAmount), '5000');
      await shotSettle(tester);
    },
  ),
  _tab(
    'console-content-editor',
    (t) => t.cMarketing,
    (t) => t.cContent,
    then: (tester, t) async {
      await tester.tap(find.text(t.cNewContent).first);
      await shotSettle(tester);
    },
  ),
  ManualShot(
    'console-growth-offers',
    build: (env) => consoleScreen(env, const GrowthSettings()),
  ),
  ManualShot(
    'console-review-reports',
    device: cardView,
    build: (env) => cardHost(
      env,
      ReviewReportQueue(
        fetch: () async => [
          {
            'booking_id': 'b1',
            'rating': 1,
            'body': '전화번호 010-0000-0000 으로 연락 주세요',
            'customer_name': '김*수',
            'pro_name': '민수 청소팀',
            'hidden': true,
            'author_blocked': false,
            'reports': [
              {'reason': 'personal_info', 'note': '전화번호 포함'},
            ],
          },
        ],
        decide: (id, action) async {},
        setAuthorBlocked: (id, blocked) async {},
      ),
    ),
  ),
  ManualShot(
    'console-cases',
    device: cardView,
    build: (env) => cardHost(
      env,
      CaseQueue(
        fetch: () async => [
          {
            'id': 'c1',
            'kind': 'no_show',
            'status': 'open',
            'booking_number': 'B-20261121-0042',
            'overdue': true,
          },
        ],
        detail: (id) async => {
          'case': {
            'id': 'c1',
            'kind': 'no_show',
            'status': 'open',
            'summary': '도착 확인 없음',
            'booking_id': 'bk-1',
          },
          'booking': {
            'id': 'bk-1',
            'checked_in_at': null,
            'checked_out_at': null,
          },
          'notes': [
            {'author': '김상담', 'body': '파트너에게 전화했어요. 20분 늦는다고 해요.'},
          ],
        },
        update: (id, body) async => const {},
        reassign: (id, body) async => const {},
        isAdmin: true,
      ),
    ),
    act: (tester, t) => _tapRow(tester, 'B-20261121-0042'),
  ),
  _tab('console-campaign-new', (t) => t.cMarketing, (t) => t.cmTab),
  _tab('console-funnel', (t) => t.cMarketing, (t) => t.anFunnel('30')),
  // 23 Rules, texts, records
  _page('console-rules', (t) => t.rulesTab),
  _page('console-notify-templates', (t) => t.ntNavLabel),
  _page(
    'console-audit',
    (t) => t.cAudit,
    then: (tester, t) => _tapRow(tester, '홍길동'),
  ),
  _page('console-renewals', (t) => t.rnwTitle),
  _page(
    'console-privacy-requests',
    (t) => t.prqTitle,
    api: (f) => f
      ..reply('GET', '/admin/privacy/customers', [
        {'id': 'u-0', 'name': '김지민', 'phone': '01012345678', 'bookings': 3},
      ])
      ..reply('GET', '/admin/privacy/requests', [
        {
          'kind': 'export',
          'status': 'done',
          'staff_name': '홍길동',
          'created_at': '2026-11-18T02:10:00Z',
        },
      ]),
    then: (tester, t) async {
      await tester.enterText(
        find.widgetWithText(TextField, t.prqPhone),
        '010-1234-5678',
      );
      await tester.tap(find.text(t.prqFind));
      await shotSettle(tester);
    },
  ),
];

void main() => runManualShots('daily', _shots, _daily);
