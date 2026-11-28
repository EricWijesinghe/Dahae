// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:ui/ui.dart';

import '../booking_labels.dart';
import '../state.dart';
import 'confirm_action.dart';
import 'inbox.dart';
import '../widgets/inline_button.dart';

/// Detail payload: the booking JSON, its quotes and the counts line (null when none).
typedef _PanelData = (Json, List<Json>, String?);

/// List + detail. Desktop shows both side by side; phones push the detail.
class BookingsScreen extends StatefulWidget {
  const BookingsScreen({super.key, this.quoteDesk = false});
  final bool quoteDesk;
  @override
  State<BookingsScreen> createState() => _BookingsScreenState();
}

class _BookingsScreenState extends State<BookingsScreen> with LiveReload {
  final _list = GlobalKey<AsyncViewState<List<Booking>>>();
  final _search = TextEditingController();
  int? _status;
  String? _service;
  String? _selected;

  @override
  void onLive() => _list.currentState?.reload();

  late final _pager = Pager<Booking>(
    context.api,
    '/admin/bookings',
    Booking.fromJson,
    query: () => {
      'status': widget.quoteDesk ? '0' : _status?.toString(),
      'service': _service,
      'q': _search.text.trim().isEmpty ? null : _search.text.trim(),
    },
  );

  void _open(Booking b) {
    if (context.isWide) {
      setState(() => _selected = b.id);
    } else {
      Navigator.of(context).push(
        MaterialPageRoute<void>(builder: (_) => BookingDetailPage(id: b.id)),
      );
    }
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final list = Column(
      children: [
        Padding(
          padding: const EdgeInsetsDirectional.fromSTEB(
            Space.md,
            Space.md,
            Space.md,
            Space.xs,
          ),
          child: Column(
            children: [
              TextField(
                controller: _search,
                decoration: InputDecoration(
                  prefixIcon: const Icon(Icons.search_rounded),
                  hintText: t.cSearch,
                  isDense: true,
                ),
                onSubmitted: (_) => _list.currentState?.reload(),
              ),
              const SizedBox(height: Space.xs),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  if (!widget.quoteDesk)
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 240),
                      child: DropdownButton<int?>(
                        isExpanded: true,
                        value: _status,
                        hint: KeepAllText(t.cAllStatus),
                        items: [
                          DropdownMenuItem(
                            value: null,
                            child: KeepAllText(t.cAllStatus),
                          ),
                          for (final s in const [0, 1, 2, 3, -1])
                            DropdownMenuItem(
                              value: s,
                              child: KeepAllText(t.status(s)),
                            ),
                        ],
                        onChanged: (v) {
                          setState(() => _status = v);
                          _list.currentState?.reload();
                        },
                      ),
                    ),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 240),
                    child: DropdownButton<String?>(
                      isExpanded: true,
                      value: _service,
                      hint: KeepAllText(t.cAllServices),
                      items: [
                        DropdownMenuItem(
                          value: null,
                          child: KeepAllText(t.cAllServices),
                        ),
                        for (final c
                            in widget.quoteDesk
                                ? const ['move_out', 'move_in']
                                : const [
                                    'studio',
                                    'kitchen',
                                    'bathroom',
                                    'mattress',
                                    'aircon',
                                    'sofa',
                                    'move_out',
                                    'move_in',
                                  ])
                          DropdownMenuItem(
                            value: c,
                            child: KeepAllText(t.service(c)),
                          ),
                      ],
                      onChanged: (v) {
                        setState(() => _service = v);
                        _list.currentState?.reload();
                      },
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        Expanded(
          child: AsyncView<List<Booking>>(
            key: _list,
            load: _pager.first,
            builder: (context, _, reload) => LoadMore<Booking>(
              pager: _pager,
              builder: (context, all) {
                final rows = widget.quoteDesk
                    ? all.where((b) => b.isQuote).toList()
                    : all;
                // Quote desk filters here: an empty filtered page keeps a list so the next page still loads.
                return rows.isEmpty && !_pager.hasMore
                    ? EmptyState(icon: Icons.inbox_rounded, title: t.cNoResults)
                    : RefreshIndicator(
                        onRefresh: reload,
                        child: ListView.separated(
                          itemCount: rows.length,
                          separatorBuilder: (_, _) => const Divider(height: 1),
                          itemBuilder: (_, i) => _Row(
                            booking: rows[i],
                            selected: rows[i].id == _selected,
                            onTap: () => _open(rows[i]),
                          ),
                        ),
                      );
              },
            ),
          ),
        ),
      ],
    );
    if (!context.isWide) return list;
    return Row(
      children: [
        SizedBox(width: 440, child: list),
        const VerticalDivider(width: 1),
        Expanded(
          child: _selected == null
              ? EmptyState(
                  icon: Icons.touch_app_rounded,
                  title: t.cSelectBooking,
                )
              : BookingPanel(
                  key: ValueKey(_selected),
                  id: _selected!,
                  onChanged: () => _list.currentState?.reload(),
                ),
        ),
      ],
    );
  }
}

class _Row extends StatelessWidget {
  const _Row({
    required this.booking,
    required this.selected,
    required this.onTap,
  });
  final Booking booking;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final b = booking;
    final t = context.t;
    final amount = b.finalPrice ?? b.listPrice ?? b.estimate;
    return ListTile(
      selected: selected,
      selectedTileColor: context.colors.primaryTint,
      dense: true,
      leading: ServiceGlyph(b.serviceCode, size: 36),
      title: Row(
        children: [
          Expanded(
            child: KeepAllText(
              '${b.number} · ${t.serviceShort(b.serviceCode)}',
              style: context.text.titleSmall,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          Flexible(
            child: StatusBadge(status: b.status, label: t.status(b.status)),
          ),
        ],
      ),
      subtitle: KeepAllText(
        '${dateLabel(b.visitDate, context.lang)} ${b.isQuote ? (b.startHour < 12 ? t.morning : t.afternoon) : hourLabel(b.startHour, context.lang)} · ${districtByCode(b.j['district_code'] as String)?.name(context.lang) ?? ''}${amount == null ? '' : ' · ${context.won(amount)}'}${b.teamName == null ? '' : ' · ${b.teamName}'}',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      onTap: onTap,
    );
  }
}

/// Full-page detail for phones and the command palette (by id or booking number).
class BookingDetailPage extends StatelessWidget {
  const BookingDetailPage({super.key, this.id, this.query});
  final String? id;
  final String? query;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: KeepAllText(query ?? context.t.detailTitle)),
    body: id != null
        ? BookingPanel(id: id!)
        : AsyncView<List<Json>>(
            load: () async =>
                (await context.api.get('/admin/bookings', query: {'q': query})
                        as List)
                    .cast<Json>(),
            builder: (context, rows, _) => rows.isEmpty
                ? EmptyState(
                    icon: Icons.search_off_rounded,
                    title: context.t.cNoResults,
                  )
                : BookingPanel(id: rows.first['id'] as String),
          ),
  );
}

class BookingPanel extends StatefulWidget {
  const BookingPanel({super.key, required this.id, this.onChanged});
  final String id;
  final VoidCallback? onChanged;
  @override
  State<BookingPanel> createState() => _BookingPanelState();
}

class _BookingPanelState extends State<BookingPanel> with LiveReload {
  final _view = GlobalKey<AsyncViewState<_PanelData>>();

  @override
  void onLive() => _view.currentState?.reload();

  Future<_PanelData> _load() async {
    final api = context.api;
    final t = context.t;
    final d = await api.get('/admin/bookings/${widget.id}') as Json;
    final booking = Booking.fromJson(d['booking'] as Json);
    final isQuote = (d['booking'] as Json)['answers'] != null;
    final quotes = isQuote
        ? (await api.get('/admin/bookings/${widget.id}/quotes') as List)
              .cast<Json>()
        : <Json>[];
    final services = booking.unitCounts.isEmpty
        ? const <Json>[]
        : (await api.get('/admin/services') as List).cast<Json>();
    return (d, quotes, bookingCountsLine(t, booking, services));
  }

  Future<void> _act(Future<void> Function() fn) async {
    try {
      await fn();
      if (!mounted) return;
      showToast(context, context.t.cDone);
      widget.onChanged?.call();
      await _view.currentState?.reload();
    } catch (e) {
      if (mounted) showToast(context, context.t.error(e));
    }
  }

  @override
  Widget build(BuildContext context) => AsyncView<_PanelData>(
    key: _view,
    load: _load,
    builder: (context, data, _) => _body(data.$1, data.$2, data.$3),
  );

  Widget _body(Json d, List<Json> quotes, String? countsLine) {
    final t = context.t;
    final b = Booking.fromJson(d['booking'] as Json);
    final customer = d['customer'] as Json;
    final payments = (d['payments'] as List).cast<Json>();
    final estimate = d['estimate_breakdown'] as Json?;
    return ListView(
      padding: const EdgeInsets.all(Space.xl),
      children: [
        Row(
          children: [
            ServiceGlyph(b.serviceCode, size: 48),
            const SizedBox(width: Space.md),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SelectableText(
                    '${b.number} · ${t.service(b.serviceCode)}',
                    style: context.text.titleLarge,
                  ),
                  KeepAllText(
                    '${dateLabel(b.visitDate, context.lang)} ${hourLabel(b.startHour, context.lang)}',
                    style: context.text.bodySmall,
                  ),
                  if (countsLine != null)
                    KeepAllText(countsLine, style: context.text.bodySmall),
                ],
              ),
            ),
            StatusBadge(status: b.status, label: t.status(b.status)),
          ],
        ),
        const SizedBox(height: Space.lg),
        Wrap(
          spacing: Space.sm,
          runSpacing: Space.sm,
          children: [
            _InfoCard(
              title: t.cCustomer,
              lines: [
                customerNameLabel(t, customer['name'] as String?),
                formatKoreanPhone('${customer['phone']}'),
                if (customer['email'] != null) '${customer['email']}',
              ],
            ),
            _InfoCard(
              title: t.visitAddress,
              lines: [
                '${b.address['road_address']} ${b.address['detail']}',
                t.pyeong('${b.address['size_pyeong']}'),
              ],
            ),
            _InfoCard(
              title: t.cPayments,
              lines: [
                for (final p in payments)
                  '${_payKind(t, p['kind'] as String)} · ${context.won((p['amount'] as num).toInt())} · ${_payStatus(t, p['status'] as String)}${(p['refunded'] as num) > 0 ? ' (−${context.won((p['refunded'] as num).toInt())})' : ''}',
                if (payments.isEmpty) '-',
              ],
            ),
            if (b.visitNotes.values.any((v) => v is String && v.isNotEmpty))
              _InfoCard(
                title: t.visitNotes,
                lines: [
                  for (final v in b.visitNotes.values)
                    if (v is String && v.isNotEmpty) v,
                ],
              ),
          ],
        ),
        const SizedBox(height: Space.lg),
        _StatusActions(booking: b, act: _act, onCancel: () => _cancel(b)),
        if (b.isQuote && b.status == 0) ...[
          const SizedBox(height: Space.lg),
          _QuoteDesk(
            booking: b,
            estimate: estimate,
            quotes: quotes,
            countsLine: countsLine,
            onSent: () => _view.currentState?.reload(),
          ),
        ],
        const SizedBox(height: Space.lg),
        if (b.threadId != null)
          OutlinedButton.icon(
            icon: const Icon(Icons.forum_rounded),
            label: KeepAllText(t.cInbox),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute<void>(
                builder: (_) => Scaffold(
                  appBar: AppBar(title: KeepAllText(b.number)),
                  body: ThreadView(threadId: b.threadId!),
                ),
              ),
            ),
          ),
      ],
    );
  }

  Future<void> _cancel(Booking b) async {
    final t = context.t;
    final amount = TextEditingController();
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: KeepAllText(t.cCancelRefund),
        content: TextField(
          controller: amount,
          keyboardType: TextInputType.number,
          inputFormatters: [FilteringTextInputFormatter.digitsOnly],
          decoration: InputDecoration(labelText: t.cRefundAmount),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: KeepAllText(t.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: ctx.colors.danger,
              // The theme's label suits the owner's fill colour, not red.
              foregroundColor: labelOnFill(ctx.colors.danger),
            ).merge(compactFilled),
            onPressed: () => Navigator.of(ctx).pop(true),
            child: KeepAllText(t.cCancelRefund),
          ),
        ],
      ),
    );
    if (ok == true && mounted) {
      final api = context.api;
      await _act(
        () => api.post('/admin/bookings/${b.id}/cancel', {
          'refund_amount': amount.text.isEmpty ? null : int.parse(amount.text),
        }, ApiClient.newIdempotencyKey()),
      );
    }
    disposeAfterClose([amount]);
  }
}

String _payKind(T t, String k) => switch (k) {
  'deposit' => t.deposit,
  'balance' => t.balance,
  _ => t.total,
};
String _payStatus(T t, String s) => switch (s) {
  'paid' => t.quotePaid,
  'refunded' => t.cRefunded,
  'partially_refunded' => t.cPartRefunded,
  _ => t.cFailed,
};

/// Status buttons + team field. Own state so live refreshes don't wipe typing,
/// and a busy flag so a double tap can't send the same change twice.
class _StatusActions extends StatefulWidget {
  const _StatusActions({
    required this.booking,
    required this.act,
    required this.onCancel,
  });
  final Booking booking;
  final Future<void> Function(Future<void> Function()) act;
  final Future<void> Function() onCancel;
  @override
  State<_StatusActions> createState() => _StatusActionsState();
}

class _StatusActionsState extends State<_StatusActions> {
  late final _team = TextEditingController(text: widget.booking.teamName ?? '');
  bool _busy = false;

  Future<void> _run(Future<void> Function() fn) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      await widget.act(fn);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _status(int status, [String? team]) {
    final api = context.api;
    return _run(
      () => api.post('/admin/bookings/${widget.booking.id}/status', {
        'status': status,
        'team_name': ?team,
      }),
    );
  }

  @override
  void dispose() {
    _team.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final b = widget.booking;
    return AppCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KeepAllText(t.cChangeStatus, style: context.text.titleSmall),
          const SizedBox(height: Space.sm),
          if (b.status == 1 && b.proId == null)
            Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _team,
                    decoration: InputDecoration(
                      labelText: t.cTeamName,
                      isDense: true,
                    ),
                  ),
                ),
                const SizedBox(width: Space.xs),
                FilledButton(
                  style: FilledButton.styleFrom(minimumSize: const Size(0, 56)),
                  onPressed: _busy ? null : () => _status(2, _team.text),
                  child: KeepAllText(t.cAssign),
                ),
              ],
            ),
          const SizedBox(height: Space.xs),
          Wrap(
            spacing: Space.xs,
            runSpacing: Space.xs,
            children: [
              if (b.status == 2 && b.proId == null)
                FilledButton.tonal(
                  style: inlineFilled,
                  // Completion has no undo: ask first.
                  onPressed: _busy
                      ? null
                      : () async {
                          if (await confirmAction(
                            context,
                            t.cMarkDone,
                            message: t.cMarkDoneAsk,
                          )) {
                            await _status(3);
                          }
                        },
                  child: KeepAllText(t.cMarkDone),
                ),
              if (b.status == 2)
                OutlinedButton(
                  onPressed: _busy ? null : () => _status(1),
                  child: KeepAllText(t.cConfirm),
                ),
              if (b.status >= 0 && b.status < 3)
                OutlinedButton(
                  style: OutlinedButton.styleFrom(
                    foregroundColor: context.colors.danger,
                  ),
                  onPressed: _busy ? null : () => _run(widget.onCancel),
                  child: KeepAllText(t.cCancelRefund),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _InfoCard extends StatelessWidget {
  const _InfoCard({required this.title, required this.lines});
  final String title;
  final List<String> lines;
  @override
  Widget build(BuildContext context) => ConstrainedBox(
    constraints: const BoxConstraints(minWidth: 220, maxWidth: 360),
    child: AppCard(
      padding: const EdgeInsets.all(Space.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          KeepAllText(title, style: context.text.labelSmall),
          const SizedBox(height: 4),
          for (final l in lines)
            SelectableText(l, style: context.text.bodyMedium),
        ],
      ),
    ),
  );
}

/// Answers, calculated estimate, partner quotes side by side, then send the final price.
class _QuoteDesk extends StatefulWidget {
  const _QuoteDesk({
    required this.booking,
    required this.estimate,
    required this.quotes,
    required this.onSent,
    this.countsLine,
  });
  final Booking booking;
  final Json? estimate;
  final List<Json> quotes;
  final VoidCallback onSent;
  final String? countsLine;
  @override
  State<_QuoteDesk> createState() => _QuoteDeskState();
}

class _QuoteDeskState extends State<_QuoteDesk> {
  String? _quoteId;
  late final _price = TextEditingController(
    text: '${widget.booking.finalPrice ?? widget.booking.estimate ?? ''}',
  );
  final _note = TextEditingController();
  bool _busy = false;

  Future<void> _send({bool override = false}) async {
    setState(() => _busy = true);
    try {
      await context.api
          .post('/admin/bookings/${widget.booking.id}/send-quote', {
            'final_price': int.parse(_price.text),
            'quote_id': _quoteId,
            'note': _note.text.isEmpty ? null : _note.text,
            if (override) 'override': true,
          });
      if (!mounted) return;
      showToast(context, context.t.cQuoteSent);
      widget.onSent();
    } on ApiException catch (e) {
      if (!mounted) return;
      // Far from the estimate: admins confirm and resend with the override; others need an admin.
      if (e.code == 'quote_override_required') {
        if (!context.console.isAdmin) {
          showToast(context, context.t.cQuoteNeedsAdmin);
        } else if (await _confirmOverride() && mounted) {
          setState(() => _busy = false);
          await _send(override: true);
        }
        return;
      }
      showToast(context, context.t.error(e));
    } catch (e) {
      if (mounted) showToast(context, context.t.error(e));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Puts the sent price back in the price field so staff can change it and send again.
  void _reviseSent() => setState(() {
    _price.text = '${widget.booking.finalPrice}';
  });

  /// Shows the typed price against the estimate before an admin overrides the ratio guard.
  Future<bool> _confirmOverride() async {
    final t = context.t;
    final price = int.parse(_price.text);
    final estimate = widget.booking.estimate ?? 0;
    final ratio = estimate > 0 ? (price / estimate).toStringAsFixed(1) : '–';
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: KeepAllText(t.cQuoteOverride),
        content: KeepAllText(
          t.cQuoteOverrideBody(
            context.won(price),
            context.won(estimate),
            ratio,
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: KeepAllText(t.cancel),
          ),
          FilledButton(
            style: compactFilled,
            onPressed: () => Navigator.of(ctx).pop(true),
            child: KeepAllText(t.cQuoteOverride),
          ),
        ],
      ),
    );
    return ok ?? false;
  }

  @override
  void dispose() {
    _price.dispose();
    _note.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final a = widget.booking.answers ?? const {};
    final e = widget.estimate;
    return AppCard(
      border: context.colors.sun,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: KeepAllText(
                  t.cQuoteDesk,
                  style: context.text.titleMedium,
                ),
              ),
              if (widget.booking.finalPrice != null)
                Chip(
                  label: KeepAllText(t.quoteSentChip),
                  visualDensity: VisualDensity.compact,
                ),
            ],
          ),
          if (widget.booking.finalPrice != null) ...[
            _SentQuote(booking: widget.booking, onRevise: _reviseSent),
            const SizedBox(height: Space.sm),
          ],
          // The server copies these booking rows into the customer's quote card.
          KeepAllText(
            '${t.quoteVisit} ${dateLabel(widget.booking.visitDate, context.lang)} ${widget.booking.startHour < 12 ? t.morning : t.afternoon} · ${t.service(widget.booking.serviceCode)}',
            style: context.text.bodySmall,
          ),
          if (widget.countsLine != null)
            KeepAllText(widget.countsLine!, style: context.text.bodySmall),
          const SizedBox(height: Space.sm),
          KeepAllText(t.cAnswers, style: context.text.labelSmall),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final (label, value) in quoteAnswerRows(t, a))
                Chip(
                  label: KeepAllText('$label: $value'),
                  visualDensity: VisualDensity.compact,
                ),
            ],
          ),
          if (e != null) ...[
            const SizedBox(height: Space.md),
            KeepAllText(t.cEstimateCalc, style: context.text.labelSmall),
            for (final (label, value) in estimateRows(t, e, context.won))
              KeepAllText('$label  $value', style: context.text.bodyMedium),
          ],
          const SizedBox(height: Space.md),
          KeepAllText(t.cProQuotes, style: context.text.labelSmall),
          if (widget.quotes.isEmpty)
            KeepAllText(t.cNoProQuotes, style: context.text.bodySmall),
          for (final q in widget.quotes)
            Padding(
              padding: const EdgeInsets.only(top: 6),
              child: OptionCard(
                title: '${q['team_name']} · ${q['pro_name']}',
                subtitle: [
                  if (q['rating'] != null)
                    '★ ${(q['rating'] as num).toStringAsFixed(1)}',
                  if ((q['note'] as String).isNotEmpty) q['note'] as String,
                  q['state'] as String,
                ].join(' · '),
                trailing: context.won((q['amount'] as num).toInt()),
                selected: _quoteId == q['id'],
                enabled: q['state'] == 'waiting' || q['state'] == 'selected',
                onTap: () => setState(() {
                  _quoteId = q['id'] as String;
                  _price.text = '${q['amount']}';
                }),
              ),
            ),
          const SizedBox(height: Space.md),
          TextField(
            controller: _price,
            keyboardType: TextInputType.number,
            inputFormatters: [FilteringTextInputFormatter.digitsOnly],
            decoration: InputDecoration(
              labelText: t.cFinalPrice,
              isDense: true,
            ),
          ),
          const SizedBox(height: Space.xs),
          TextField(
            controller: _note,
            maxLength: 300,
            decoration: InputDecoration(labelText: t.cQuoteNote, isDense: true),
          ),
          PrimaryButton(
            label: t.cSendQuote,
            busy: _busy,
            onPressed: () => int.tryParse(_price.text) == null ? null : _send(),
          ),
        ],
      ),
    );
  }
}

/// The quote already sent for this booking: amount, sent time, resend and revise (K-04).
class _SentQuote extends StatelessWidget {
  const _SentQuote({required this.booking, required this.onRevise});
  final Booking booking;
  final VoidCallback onRevise;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final amount = context.won(booking.finalPrice!);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        KeepAllText(
          t.quoteSentEntry(amount, timeLabel(booking.updatedAt, context.lang)),
          style: context.text.labelMedium,
        ),
        Row(
          children: [
            ResendQuoteButton(bookingId: booking.id, label: t.quoteResend),
            TextButton(onPressed: onRevise, child: KeepAllText(t.quoteRevise)),
          ],
        ),
      ],
    );
  }
}
