import 'dart:math' as math;

import 'package:core/core.dart';
import 'package:test_support/test_support.dart';

/// Registers the endpoints [AppState.loadAll] and the Shell tabs call on start-up.
/// The first matching route wins, so register a test-specific override before calling this.
void stockCustomerApi(
  FakeApi api, {
  String name = '지민',
  List<Json>? addresses,
  List<Json>? coupons,
  List<Json>? methods,
  List<Json>? bookings,
  List<Json>? threads,
  List<Json>? feed,
}) {
  api
    ..reply('GET', '/me', {'name': name, 'phone': '01012345678', 'email': null})
    ..reply('PATCH', '/me', {
      'name': name,
      'phone': '01012345678',
      'email': null,
    })
    ..reply('GET', '/services', catalogJson())
    ..reply('GET', '/me/addresses', addresses ?? [addressJson()])
    ..reply('GET', '/me/coupons', coupons ?? const <Json>[])
    ..reply('GET', '/me/payment-methods', methods ?? [paymentMethodJson()])
    ..reply('GET', '/chat/threads', threads ?? const <Json>[])
    ..reply('GET', '/bookings', bookings ?? const <Json>[])
    ..reply('GET', '/content', [
      contentJson(
        kind: 'notice',
        title: const {'ko': '점검 안내', 'en': 'Maintenance'},
      ),
    ])
    ..reply('GET', '/me/notifications', const <Json>[])
    ..reply('GET', '/me/points/ledger', const <Json>[])
    ..reply('GET', '/checkouts', const <Json>[])
    ..reply('GET', '/feed', {'cards': feed ?? const <Json>[]})
    ..reply('GET', '/me/feed', {'cards': feed ?? const <Json>[]})
    ..onPost(
      '/bookings/price',
      (c) => stockPrice(c.json, coupons ?? const <Json>[]),
    );
}

/// POST /bookings/price as the server answers with peak and the welcome offer off: the
/// catalogue price less the chosen coupon. Tests of the server's extras register their own reply.
Json stockPrice(Json body, List<Json> coupons) {
  final service = Service.fromJson(serviceJson(body['service_code'] as String));
  final list = switch (service.type) {
    PricingType.fixed => service.price!,
    PricingType.counts => service.countsPrice([
      for (final n in body['counts'] as List) n as int,
    ]),
    _ => service.units[body['unit_index'] as int].price,
  };
  final coupon = coupons.where((c) => c['id'] == body['coupon_id']).firstOrNull;
  final discount = math.min(list, (coupon?['amount'] as int?) ?? 0);
  return {
    'list_price': list,
    'peak_surcharge': 0,
    'multi_discount': 0,
    'discount': discount,
    'welcome': false,
    'total': list - discount,
  };
}
