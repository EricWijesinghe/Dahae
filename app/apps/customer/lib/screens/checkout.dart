import 'dart:async';
import 'dart:math' as math;

import 'package:core/auth_pay.dart';
import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import 'booking_detail.dart';
import 'auth.dart' show ensureCustomerName;
import 'booking_flow.dart';
import 'cancel_policy_card.dart';
import 'more.dart' show LegalScreen;
import 'pay_flow.dart';
import 'pay_method_picker.dart';
import 'virtual_account.dart';

const _actionShare = 0.35;
const _priceChanged = 'price_changed';

int _int(Object? v) => (v as num? ?? 0).toInt();

/// Discount and payment (design p12): coupon, points, method, breakdown, agreement, pay.
class CheckoutScreen extends StatefulWidget {
  const CheckoutScreen({super.key, required this.draft});
  final BookingDraft draft;
  @override
  State<CheckoutScreen> createState() => _CheckoutScreenState();
}

class _CheckoutScreenState extends State<CheckoutScreen> {
  BookingDraft get d => widget.draft;
  bool _busy = false;
  bool _autoPicked = false;
  PayChoice? _choice;

  /// GET /me/points. Null until loaded, and when the call fails: the points section then stays hidden.
  Json? _wallet;

  /// "포인트 우선 사용": apply as many points as allowed. The server re-checks balance and cap.
  bool _usePoints = false;

  /// POST /bookings/price for the draft as it is now (peak, coupon or welcome offer): every amount
  /// shown and paid comes from it. Null while it loads; Pay waits for it.
  Json? _price;
  bool _priceFailed = false;

  void _retryPrice() {
    setState(() => _priceFailed = false);
    _loadPrice();
  }

  /// Synchronous part sets no state: the first call runs inside build, right after the coupon pick.
  Future<void> _loadPrice() async {
    final coupon = d.couponId;
    _price = null;
    _priceFailed = false;
    try {
      final p =
          await context.appRead.api.post('/bookings/price', _bookingBody())
              as Json;
      if (mounted && d.couponId == coupon) setState(() => _price = p);
    } on Object catch (e) {
      if (!mounted || d.couponId != coupon) return;
      setState(() => _priceFailed = true);
      showToast(context, context.t.payError(e));
    }
  }

  Future<void> _loadWallet() async {
    try {
      final w = await context.appRead.api.get('/me/points') as Json;
      if (mounted) setState(() => _wallet = w);
    } on Exception {
      // Paying without points still works; the section is simply not offered.
    }
  }

  int get _balance => (_wallet?['balance'] as num? ?? 0).toInt();
  bool get _pointsOn => _wallet?['enabled'] == true;

  /// Most points this payment accepts: balance, the programme cap, and at least 1 won left for the PG.
  int _maxPoints(int total) {
    if (!_pointsOn) return 0;
    final cap = total * (_wallet!['max_use_bps'] as num? ?? 0).toInt() ~/ 10000;
    return math.max(0, math.min(_balance, math.min(cap, total - 1)));
  }

  int _discount(List<Coupon> coupons) {
    final c = coupons.where((c) => c.id == d.couponId).firstOrNull;
    return c == null ? 0 : (c.amount < d.listPrice ? c.amount : d.listPrice);
  }

  /// Booking body for checkout channels: the draft without the saved-method id.
  Json _bookingBody() => d.toJson()..remove('payment_method_id');

  /// Saved method -> POST /bookings (one-tap). Checkout channel -> PG window, then confirm.
  /// Both reuse one Idempotency-Key per draft, so a retry after a lost reply never charges twice.
  /// [total] is the server's price shown: if it changed since, the server refuses (`price_changed`).
  Future<void> _pay(int total, int points) async {
    final choice = _choice;
    if (choice == null || _busy) return;
    if (!await ensureCustomerName(context) || !mounted) return;
    setState(() => _busy = true);
    final app = context.appRead;
    try {
      final CheckoutResult result;
      if (choice case ChannelChoice(:final channel)) {
        result = await payWithChannel(context, channel, {
          'kind': 'booking',
          'booking': _bookingBody(),
          'expected_total': total,
          if (points > 0) 'points': points,
        });
      } else {
        final b = Booking.fromJson(
          await app.api.postAction('/bookings', {
                ...d.toJson(),
                'expected_total': total,
              })
              as Json,
        );
        result = CheckoutPaid(b.id, b);
      }
      if (!mounted) return;
      switch (result) {
        case CheckoutPaid(:final booking, :final bookingId):
          app.bookingsChanged();
          app.analytics.track('payment_done', screen: 'checkout');
          await app.loadCoupons();
          if (!mounted) return;
          await Navigator.of(context).pushAndRemoveUntil(
            MaterialPageRoute<void>(
              builder: (_) => booking == null
                  ? BookingDetailScreen(id: bookingId)
                  : DoneScreen(booking: booking),
            ),
            (r) => r.isFirst,
          );
        case CheckoutPending():
          // Possibly charged: leave the checkout for good so Pay is never offered again.
          app.bookingsChanged();
          Navigator.of(context).popUntil((r) => r.isFirst);
        case CheckoutAwaitingDeposit(:final account):
          app.bookingsChanged();
          await Navigator.of(context).pushAndRemoveUntil(
            MaterialPageRoute<void>(
              builder: (_) => VirtualAccountScreen(account: account),
            ),
            (r) => r.isFirst,
          );
        case CheckoutFailed(:final code):
          showToast(context, _failure(code));
          if (code == _priceChanged) unawaited(_loadPrice());
        case CheckoutCancelled():
          break;
      }
    } on ApiException catch (e) {
      if (!mounted) return;
      showToast(context, _failure(e.code));
      if (e.code == _priceChanged) unawaited(_loadPrice());
    } catch (e) {
      if (mounted) showToast(context, context.t.payError(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  String _failure(String code) => code == _priceChanged
      ? context.t.checkoutPriceChanged
      : context.t.checkoutFailure(code);

  void _choose(PayChoice c) => setState(() {
    _choice = c;
    d.methodId = switch (c) {
      SavedChoice(:final methodId) => methodId,
      ChannelChoice() => null,
    };
  });

  Future<void> _pickMethod() async {
    final savedId = switch (_choice) {
      SavedChoice(:final methodId) => methodId,
      _ => null,
    };
    final channelCode = switch (_choice) {
      ChannelChoice(:final channel) => channel.code,
      _ => null,
    };
    await showAppSheet<void>(
      context,
      title: context.t.dfPayMethod,
      builder: (ctx) => SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(
          Space.gutter,
          0,
          Space.gutter,
          Space.gutter,
        ),
        child: PayMethodPicker(
          selectedSavedId: savedId,
          selectedChannel: channelCode,
          onChoose: (c) {
            _choose(c);
            Navigator.of(ctx).pop();
          },
        ),
      ),
    );
  }

  String _methodLabel(T t, AppState app) => switch (_choice) {
    SavedChoice(:final methodId) =>
      app.methods.where((m) => m.id == methodId).firstOrNull?.displayName ??
          t.dfPayMethod,
    ChannelChoice(:final channel) => t.payMethod(channel.code),
    null => t.needMethod,
  };

  @override
  Widget build(BuildContext context) {
    final app = context.app;
    final t = context.t;
    final paused = RemoteConfigScope.of(context).maintenance;
    final usable =
        app.coupons.where((c) => c.appliesTo(d.service.code)).toList()
          ..sort((a, b) => b.amount.compareTo(a.amount));
    if (!_autoPicked) {
      // Best coupon pre-applied: customers shouldn't have to hunt for savings.
      d.couponId ??= usable.firstOrNull?.id;
      d.methodId ??= app.methods.firstOrNull?.id;
      _choice = d.methodId == null ? null : SavedChoice(d.methodId!);
      _autoPicked = true;
      _loadWallet();
      _loadPrice();
    }
    final price = _price;
    // Until the server answers, the catalogue figures only hold the layout: Pay stays busy.
    final peak = _int(price?['peak_surcharge']);
    final base = price == null
        ? d.listPrice
        : _int(price['list_price']) - peak - _int(price['multi_discount']);
    final discount = price == null
        ? _discount(app.coupons)
        : _int(price['discount']);
    final welcome = price?['welcome'] == true;
    final total = price == null ? d.listPrice - discount : _int(price['total']);
    // Points go through the payment window only; a saved card charges the full total in one tap.
    final viaWindow = _choice is ChannelChoice;
    final maxPoints = _maxPoints(total);
    final points = viaWindow && _usePoints ? maxPoints : 0;
    final due = total - points;
    final coupon = app.coupons.where((c) => c.id == d.couponId).firstOrNull;
    return Scaffold(
      backgroundColor: context.colors.surface,
      appBar: AppBar(
        backgroundColor: context.colors.surface,
        leading: const CloseButton(),
      ),
      body: ListView(
        children: [
          FlowTitle(t.dfPayTitle),
          FlowSection(
            title: t.coupon,
            children: [
              _IconRow(
                icon: 'ui_coupon',
                title: coupon == null
                    ? t.couponNone
                    : '−${context.won(discount)}',
                subtitle: coupon == null
                    ? (usable.isEmpty
                          ? null
                          : t.couponAvailable('${usable.length}'))
                    : t.couponName(coupon.name),
                action: usable.isEmpty
                    ? null
                    : (t.dfChange, () => _pickCoupon(usable)),
              ),
            ],
          ),
          if (_pointsOn) ...[
            const FlowBand(),
            FlowSection(
              title: t.dfPoints,
              trailing: _PointsFirst(
                value: points > 0,
                onChanged: viaWindow && maxPoints > 0
                    ? (v) => setState(() => _usePoints = v)
                    : null,
              ),
              children: [
                _IconRow(
                  icon: 'ui_points',
                  title: points > 0
                      ? '−${t.walletPoints('$points')}'
                      : t.dfPointsOff,
                  subtitle: !viaWindow
                      ? t.checkoutPointsNeedsWindow
                      : maxPoints == 0
                      ? t.dfPointsNone
                      : t.checkoutPointsAvail('$_balance', '$maxPoints'),
                ),
              ],
            ),
          ],
          const FlowBand(),
          FlowSection(
            title: t.dfPayMethod,
            children: [
              _IconRow(
                icon: 'pay_card',
                title: _methodLabel(t, app),
                action: (t.dfChange, _pickMethod),
              ),
            ],
          ),
          const FlowBand(),
          FlowSection(
            children: [
              Theme(
                data: Theme.of(
                  context,
                ).copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                  initiallyExpanded: true,
                  tilePadding: EdgeInsets.zero,
                  childrenPadding: EdgeInsets.zero,
                  title: Text(
                    [
                      t.service(d.service.code),
                      if (d.service.isCounts)
                        t.unitCounts(d.service, d.counts)
                      else if (d.unitIndex != null)
                        t.unit(d.service.units[d.unitIndex!].labelKey),
                    ].join(' · '),
                    style: context.text.titleMedium?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                  children: [
                    // The points line slides in when the wallet loads; changed values fade.
                    AnimatedSize(
                      duration: motionFor(context, Motion.standard),
                      curve: Motion.ease,
                      alignment: Alignment.topCenter,
                      child: Column(
                        children: [
                          InfoRow(
                            t.dfBaseAmount(t.duration(d.minutes)),
                            context.won(base),
                          ),
                          if (peak > 0)
                            InfoRow(t.qaPeak, '+${context.won(peak)}'),
                          InfoRow(
                            welcome ? t.welcomeOfferTitle : t.dfCouponDiscount,
                            discount == 0
                                ? context.won(0)
                                : '−${context.won(discount)}',
                            valueStyle: context.text.bodyMedium?.copyWith(
                              color: context.primary,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          if (_pointsOn)
                            InfoRow(
                              t.dfPoints,
                              points == 0
                                  ? context.won(0)
                                  : '−${t.walletPoints('$points')}',
                            ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              const Divider(height: Space.lg),
              // A Wrap: at large text the amount drops under its label, and it only
              // scales down when it alone is wider than the row.
              Wrap(
                alignment: WrapAlignment.spaceBetween,
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: Space.sm,
                children: [
                  Text(t.dfDue, style: context.text.titleMedium),
                  FittedBox(
                    fit: BoxFit.scaleDown,
                    child: RollingAmount(
                      value: due,
                      format: context.won,
                      style: context.text.headlineSmall?.copyWith(
                        fontWeight: FontWeight.w800,
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: Space.md),
              AgreementBox(total: total),
            ],
          ),
        ],
      ),
      bottomNavigationBar: BottomActionBar(
        child: PrimaryButton(
          label: _choice == null ? t.needMethod : t.payAmount(context.won(due)),
          busy: _busy || (_choice != null && price == null && !_priceFailed),
          // Maintenance pauses new bookings: the pay button stays off until the owner switches it off.
          // Without the server's price, a tap asks for it again instead of paying.
          onPressed: _choice == null
              ? _pickMethod
              : paused
              ? null
              : price == null
              ? (_priceFailed ? _retryPrice : null)
              : () => _pay(total, points),
        ),
      ),
    );
  }

  Future<void> _pickCoupon(List<Coupon> usable) async {
    final t = context.t;
    final picked = await showAppSheet<String>(
      context,
      title: t.coupon,
      builder: (ctx) => ListView(
        shrinkWrap: true,
        padding: const EdgeInsets.fromLTRB(
          Space.gutter,
          0,
          Space.gutter,
          Space.gutter,
        ),
        children: [
          for (final c in usable)
            Padding(
              padding: const EdgeInsets.only(bottom: Space.xs),
              child: OptionCard(
                title: t.couponName(c.name),
                subtitle: t.couponUntil(dateLabel(c.expiresAt, ctx.lang)),
                trailing: ctx.won(c.amount),
                selected: c.id == d.couponId,
                onTap: () => Navigator.of(ctx).pop(c.id),
              ),
            ),
          OptionCard(
            title: t.couponNone,
            selected: d.couponId == null,
            onTap: () => Navigator.of(ctx).pop(''),
          ),
        ],
      ),
    );
    if (picked != null) {
      setState(() => d.couponId = picked.isEmpty ? null : picked);
      unawaited(_loadPrice());
    }
  }
}

class _PointsFirst extends StatelessWidget {
  const _PointsFirst({required this.value, required this.onChanged});
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  // The section header caps its trailing widget's width, so the label wraps when it must.
  Widget build(BuildContext context) => MergeSemantics(
    child: InkWell(
      onTap: onChanged == null ? null : () => onChanged!(!value),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Checkbox(
            value: value,
            onChanged: onChanged == null ? null : (v) => onChanged!(v ?? false),
            shape: const CircleBorder(),
          ),
          Flexible(
            child: Text(
              context.t.dfPointsFirst,
              style: context.text.bodyMedium,
            ),
          ),
        ],
      ),
    ),
  );
}

/// 3D icon, title/subtitle and an optional "변경" action (coupon, points, method rows).
class _IconRow extends StatelessWidget {
  const _IconRow({
    required this.icon,
    required this.title,
    this.subtitle,
    this.action,
  });
  final String icon;
  final String title;
  final String? subtitle;
  final (String, VoidCallback)? action;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      ExcludeSemantics(child: Icon3D(icon, size: 44)),
      const SizedBox(width: Space.sm),
      Expanded(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // An applied coupon, points or another method fades in over the old line.
            AnimatedSwitcher(
              duration: motionFor(context, Motion.standard),
              layoutBuilder: (current, previous) => Stack(
                alignment: AlignmentDirectional.centerStart,
                children: [...previous, ?current],
              ),
              child: Text(
                title,
                key: ValueKey(title),
                style: context.text.titleSmall?.copyWith(
                  fontWeight: FontWeight.w700,
                ),
              ),
            ),
            if (subtitle != null)
              Text(
                subtitle!,
                style: context.text.bodySmall?.copyWith(
                  color: context.colors.inkMuted,
                ),
              ),
          ],
        ),
      ),
      if (action case (final label, final onTap))
        capped(
          context,
          _actionShare,
          TextButton(
            onPressed: onTap,
            child: Text(
              label,
              textAlign: TextAlign.center,
              style: TextStyle(
                color: context.primary,
                fontWeight: FontWeight.w700,
              ),
            ),
          ),
        ),
    ],
  );
}

/// Free-cancel line, 결제대행(PG) notice, and links to the terms and the cancel/refund rules,
/// right above the pay button.
class AgreementBox extends StatelessWidget {
  const AgreementBox({super.key, required this.total});
  final int total;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final small = context.text.bodySmall?.copyWith(
      color: context.colors.inkSoft,
    );
    return Container(
      padding: const EdgeInsets.all(Space.md),
      decoration: BoxDecoration(
        color: context.colors.surfaceHigh,
        borderRadius: BorderRadius.circular(Radii.tile),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(t.dfCancelNote, style: small),
          Text(t.dfPgNotice, style: small),
          Wrap(
            spacing: Space.xs,
            children: [
              TextButton(
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) =>
                        LegalScreen(slug: 'terms', title: t.moreTerms),
                  ),
                ),
                child: Text(t.moreTerms),
              ),
              TextButton(
                onPressed: () => showAppSheet<void>(
                  context,
                  title: t.dfRefundPolicy,
                  builder: (_) => SingleChildScrollView(
                    padding: const EdgeInsets.fromLTRB(
                      Space.gutter,
                      0,
                      Space.gutter,
                      Space.gutter,
                    ),
                    child: CancelPolicyCard(total: total),
                  ),
                ),
                child: Text(t.dfRefundPolicy),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
