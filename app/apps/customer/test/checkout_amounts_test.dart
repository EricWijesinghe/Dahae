import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:customer/screens/booking_flow.dart';
import 'package:customer/screens/pay_flow.dart';
import 'package:customer/screens/pending_deposits_banner.dart';
import 'package:customer/screens/virtual_account.dart';
import 'package:customer/state.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:test_support/test_support.dart';
import 'package:ui/ui.dart';

import 'support/customer_host.dart';
import 'support/customer_routes.dart';

/// The server's figures for the studio (45,000 in the catalogue).
Json _price({int peak = 0, int discount = 0, bool welcome = false}) => {
  'list_price': 45000 + peak,
  'peak_surcharge': peak,
  'multi_discount': 0,
  'discount': discount,
  'welcome': welcome,
  'total': 45000 + peak - discount,
};

String _pay(int won) => koT.payAmount(formatWon(won, 'ko'));

/// Opens the checkout for a studio visit with the given server price and saved card or channel.
Future<FakeApi> _checkout(
  WidgetTester tester, {
  Object? Function(ApiCall)? price,
  List<Json> coupons = const [],
  bool savedCard = true,
  Json? wallet,
  void Function(FakeApi)? routes,
}) async {
  setViewport(tester, tallPhoneSize);
  final fake = FakeApi();
  routes?.call(fake);
  if (price != null) fake.onPost('/bookings/price', price);
  fake
    ..reply('GET', '/me/points', wallet ?? {'enabled': false})
    ..reply('GET', '/payments/methods', [
      {'code': 'transfer', 'flow': 'checkout'},
    ])
    ..reply('POST', '/bookings', bookingJson(id: 'bk-new'))
    // Refused on purpose: the test reads the request, the PG window is not part of it.
    ..reply('POST', '/checkouts', {
      'error': {'code': 'points_insufficient'},
    }, status: 409);
  stockCustomerApi(
    fake,
    addresses: [addressJson(sizePyeong: 8)],
    coupons: coupons,
    methods: savedCard ? [paymentMethodJson()] : const <Json>[],
  );
  final env = await TestEnv.create(fake: fake);
  final app = AppState(env.api, FakeRealtime(env.api));
  await app.loadAll();
  final d = BookingDraft(app.service('studio')!)
    ..address = app.addresses.first
    ..date = DateUtils.dateOnly(DateTime.now()).add(const Duration(days: 5))
    ..hour = 9;
  await tester.pumpWidget(
    CustomerTestHost(
      env: env,
      app: app,
      home: CheckoutScreen(draft: d),
    ),
  );
  await settle(tester);
  return fake;
}

Future<void> _tap(WidgetTester tester, Finder f) async {
  await tester.ensureVisible(f);
  await tester.pump();
  await tester.tap(f);
  await settle(tester);
}

void main() {
  testWidgets(
    'peak surcharge: the server price is shown, charged and sent back as expected_total',
    (tester) async {
      final fake = await _checkout(tester, price: (_) => _price(peak: 4500));

      expect(find.text(koT.qaPeak), findsOneWidget);
      expect(find.text('+${formatWon(4500, 'ko')}'), findsOneWidget);
      expect(find.text(formatWon(45000, 'ko')), findsOneWidget);
      await _tap(tester, find.text(_pay(49500)));

      final body = fake.lastCall('POST', '/bookings')!.json;
      expect(body['expected_total'], 49500);
      expect(body['payment_method_id'], 'pm-1');
      final asked = fake.lastCall('POST', '/bookings/price')!.json;
      expect(asked.containsKey('payment_method_id'), isFalse);
      expect(asked['start_hour'], 9);
      await unmount(tester);
    },
  );

  testWidgets('welcome offer: its own discount line and the server total', (
    tester,
  ) async {
    await _checkout(
      tester,
      price: (_) => _price(discount: 5000, welcome: true),
    );

    expect(find.text(koT.welcomeOfferTitle), findsOneWidget);
    expect(find.text(koT.dfCouponDiscount), findsNothing);
    expect(find.text('−${formatWon(5000, 'ko')}'), findsOneWidget);
    expect(find.text(_pay(40000)), findsOneWidget);
    await unmount(tester);
  });

  testWidgets(
    'coupon: the price is asked again for each pick and its discount shown',
    (tester) async {
      final fake = await _checkout(
        tester,
        coupons: [couponJson(id: 'cp-1', name: 'any', amount: 10000)],
      );
      expect(
        fake.lastCall('POST', '/bookings/price')!.json['coupon_id'],
        'cp-1',
      );
      expect(find.text(_pay(35000)), findsOneWidget);

      await _tap(tester, find.text(koT.dfChange).first);
      await _tap(tester, find.text(koT.couponNone).last);
      expect(
        fake.lastCall('POST', '/bookings/price')!.json['coupon_id'],
        isNull,
      );
      expect(find.text(_pay(45000)), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('points come off the server total, which is sent with them', (
    tester,
  ) async {
    final fake = await _checkout(
      tester,
      savedCard: false,
      price: (_) => _price(peak: 4500),
      wallet: {
        'enabled': true,
        'balance': 8000,
        'max_use_bps': 3000,
        'entries': const <Json>[],
      },
    );
    await _tap(tester, find.text(koT.needMethod).last);
    await _tap(tester, find.text(koT.payMethod('transfer')).last);
    await _tap(tester, find.byType(Checkbox));
    final pay = find.text(_pay(41500));
    expect(pay, findsOneWidget);

    await _tap(tester, pay);
    final body = fake.lastCall('POST', '/checkouts')!.json;
    expect((body['expected_total'], body['points']), (49500, 8000));
    await unmount(tester);
  });

  testWidgets(
    'a price changed since it was shown: the customer is told and sees the new one',
    (tester) async {
      var peak = 0;
      final fake = await _checkout(
        tester,
        price: (_) => _price(peak: peak),
        routes: (f) => f.onPost('/bookings', (_) {
          peak = 4500;
          return const ApiReply(409, {
            'error': {'code': 'price_changed'},
          });
        }),
      );
      await _tap(tester, find.text(_pay(45000)));

      expect(find.text(koT.checkoutPriceChanged), findsOneWidget);
      expect(fake.callsTo('POST', '/bookings/price'), hasLength(2));
      expect(find.text(_pay(49500)), findsOneWidget);
      await unmount(tester);
    },
  );

  testWidgets('no server price: pay stays off and a tap asks for it again', (
    tester,
  ) async {
    var fail = true;
    final fake = await _checkout(
      tester,
      price: (_) => fail
          ? const ApiReply(503, {
              'error': {'code': 'unavailable'},
            })
          : _price(),
    );
    final button = find.widgetWithText(PrimaryButton, _pay(45000));
    expect(fake.callsTo('POST', '/bookings'), isEmpty);
    fail = false;
    await _tap(tester, button);
    expect(fake.callsTo('POST', '/bookings'), isEmpty, reason: 'retry only');
    expect(fake.callsTo('POST', '/bookings/price'), hasLength(2));
    await _tap(tester, button);
    expect(fake.lastCall('POST', '/bookings')!.json['expected_total'], 45000);
    await unmount(tester);
  });

  group('virtual account', () {
    /// As the server sends it: the checkout total was 45,000 and 8,000 points went on it.
    final server = {
      'bank': 'KOOKMIN',
      'account_number': '123-456-789012',
      'holder': '다해',
      'due': '2030-01-02T14:59:00Z',
      'amount': 37000,
    };

    testWidgets(
      'opened later (chat, booking detail) shows the amount the server expects',
      (tester) async {
        setViewport(tester, phoneSize);
        final fake = FakeApi();
        stockCustomerApi(fake);
        final env = await TestEnv.create(fake: fake);
        final app = AppState(env.api, FakeRealtime(env.api));
        await tester.pumpWidget(
          CustomerTestHost(
            env: env,
            app: app,
            home: Builder(
              builder: (context) => TextButton(
                onPressed: () => showVirtualAccount(
                  context,
                  VirtualAccount.fromJson(server),
                ),
                child: const Text('open'),
              ),
            ),
          ),
        );
        await _tap(tester, find.text('open'));

        expect(find.byType(VirtualAccountScreen), findsOneWidget);
        expect(find.text(koT.apVaAmount), findsOneWidget);
        expect(find.text(formatWon(37000, 'ko')), findsOneWidget);
        expect(find.text('123-456-789012'), findsOneWidget);
        await unmount(tester);
      },
    );

    testWidgets(
      'bookings banner: the deposit is the account amount, not the checkout total',
      (tester) async {
        setViewport(tester, phoneSize);
        final fake = FakeApi()
          ..reply('GET', '/checkouts', [
            {'id': 'co-1', 'amount': 45000, 'virtual_account': server},
          ]);
        stockCustomerApi(fake);
        final env = await TestEnv.create(fake: fake);
        final app = AppState(env.api, FakeRealtime(env.api));
        await tester.pumpWidget(
          CustomerTestHost(
            env: env,
            app: app,
            home: const Scaffold(body: PendingDepositsBanner()),
          ),
        );
        await settle(tester);
        expect(find.textContaining(formatWon(37000, 'ko')), findsOneWidget);
        expect(find.textContaining(formatWon(45000, 'ko')), findsNothing);

        await _tap(tester, find.text(koT.apVaBannerTitle));
        expect(find.text(formatWon(37000, 'ko')), findsOneWidget);
        await unmount(tester);
      },
    );
  });
}
