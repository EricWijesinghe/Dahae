// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import '../state.dart';
import 'subscribe.dart';

/// Regular cleaning subscriptions: the customer's list. Opens [SubscribeScreen] to start one
/// and [SubscriptionDetailScreen] to manage one.
class SubscriptionsScreen extends StatefulWidget {
  const SubscriptionsScreen({super.key});

  @override
  State<SubscriptionsScreen> createState() => _SubscriptionsScreenState();
}

class _SubscriptionsScreenState extends State<SubscriptionsScreen> {
  late Future<List<Json>> _list;

  @override
  void initState() {
    super.initState();
    _list = _load();
  }

  Future<List<Json>> _load() async {
    final rows = await context.api.get('/subscriptions') as List;
    return rows.cast<Json>();
  }

  void _reload() => setState(() => _list = _load());

  Future<void> _new() async {
    final created = await Navigator.of(
      context,
    ).push<bool>(MaterialPageRoute(builder: (_) => const SubscribeScreen()));
    if (created == true) _reload();
  }

  Future<void> _open(Json sub) async {
    await Navigator.of(context).push<void>(
      MaterialPageRoute(
        builder: (_) => SubscriptionDetailScreen(id: sub['id'] as String),
      ),
    );
    _reload();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return Scaffold(
      appBar: AppBar(title: KeepAllText(t.subTitle)),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: _new,
        icon: const Icon(Icons.add),
        label: KeepAllText(t.subNew),
      ),
      body: FutureBuilder<List<Json>>(
        future: _list,
        builder: (context, snap) {
          if (snap.connectionState != ConnectionState.done) {
            return const Center(child: CircularProgressIndicator());
          }
          if (snap.hasError) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(Space.xl),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    KeepAllText(t.cSubsLoadFailed),
                    TextButton(onPressed: _reload, child: KeepAllText(t.retry)),
                  ],
                ),
              ),
            );
          }
          final rows = snap.data ?? const <Json>[];
          if (rows.isEmpty) {
            return Center(
              child: Padding(
                padding: const EdgeInsets.all(Space.xl),
                child: KeepAllText(t.subListEmpty),
              ),
            );
          }
          return ListView.separated(
            padding: const EdgeInsets.all(Space.xl),
            itemCount: rows.length,
            separatorBuilder: (_, _) => const SizedBox(height: Space.md),
            itemBuilder: (context, i) {
              final sub = rows[i];
              return Card(
                child: ListTile(
                  title: KeepAllText(subscriptionTitle(t, sub)),
                  subtitle: KeepAllText(
                    subscriptionStatus(t, sub['status'] as String),
                  ),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _open(sub),
                ),
              );
            },
          );
        },
      ),
    );
  }
}

/// "매주 · 수 09:00" style summary, built from the cadence and the weekday/hour the server stored.
String subscriptionTitle(T t, Json sub) {
  final cadence = cadenceLabel(t, sub['cadence_weeks'] as int);
  final hour = sub['start_hour'] as int;
  return '$cadence · ${hour.toString().padLeft(2, '0')}:00';
}

String subscriptionStatus(T t, String status) => switch (status) {
  'paused' => t.subStatusPaused,
  'cancelled' => t.subStatusCancelled,
  _ => t.subStatusActive,
};

/// One subscription: status, pause or resume, cancel, and per-visit skip or move.
class SubscriptionDetailScreen extends StatefulWidget {
  const SubscriptionDetailScreen({super.key, required this.id});

  final String id;

  @override
  State<SubscriptionDetailScreen> createState() =>
      _SubscriptionDetailScreenState();
}

class _SubscriptionDetailScreenState extends State<SubscriptionDetailScreen> {
  Json? _view;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final v = await context.api.get('/subscriptions/${widget.id}') as Json;
      if (mounted) setState(() => _view = v);
    } on ApiException {
      if (mounted) setState(() => _failed = true);
    }
  }

  /// Runs one action, shows the server's error if it refuses, and refreshes the view.
  Future<void> _act(String path, [Object? body]) async {
    try {
      final v =
          await context.api.post('/subscriptions/${widget.id}/$path', body)
              as Json;
      if (mounted) setState(() => _view = v);
    } on ApiException {
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: KeepAllText(context.t.errorGeneric)));
      }
    }
  }

  Future<void> _cancel() async {
    final t = context.t;
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        content: KeepAllText(t.subCancelConfirm),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: KeepAllText(t.cancel),
          ),
          TextButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: KeepAllText(t.subCancelSub),
          ),
        ],
      ),
    );
    if (ok == true) await _act('cancel');
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final view = _view;
    final sub = view?['subscription'] as Json?;
    return Scaffold(
      appBar: AppBar(title: KeepAllText(t.subTitle)),
      body: _failed
          ? Center(
              child: TextButton(onPressed: _load, child: KeepAllText(t.retry)),
            )
          : sub == null
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(Space.xl),
              children: [
                KeepAllText(subscriptionTitle(t, sub), style: context.text.titleLarge),
                const SizedBox(height: Space.sm),
                KeepAllText(subscriptionStatus(t, sub['status'] as String)),
                if (sub['status'] == 'paused' &&
                    sub['paused_reason'] == 'payment') ...[
                  const SizedBox(height: Space.sm),
                  KeepAllText(t.subPausedPayment),
                ],
                const SizedBox(height: Space.lg),
                if (sub['status'] == 'active')
                  OutlinedButton(
                    key: const Key('sub-pause'),
                    onPressed: () => _act('pause'),
                    child: KeepAllText(t.subPause),
                  ),
                if (sub['status'] == 'paused')
                  FilledButton(
                    key: const Key('sub-resume'),
                    onPressed: () => _act('resume', const <String, Object?>{}),
                    child: KeepAllText(t.subResume),
                  ),
                if (sub['status'] != 'cancelled') ...[
                  const SizedBox(height: Space.sm),
                  TextButton(
                    key: const Key('sub-cancel'),
                    onPressed: _cancel,
                    child: KeepAllText(t.subCancelSub),
                  ),
                ],
                const SizedBox(height: Space.lg),
                for (final v
                    in (view?['visits'] as List? ?? const []).cast<Json>())
                  ListTile(
                    key: Key('visit-${v['id']}'),
                    title: KeepAllText(v['visit_date'] as String),
                    subtitle: KeepAllText(_visitState(t, v['state'] as String)),
                    trailing: v['state'] == 'planned' || v['state'] == 'booked'
                        ? TextButton(
                            key: Key('skip-${v['id']}'),
                            onPressed: () => _act(
                              'visits/${v['id']}/skip',
                              const <String, Object?>{},
                            ),
                            child: KeepAllText(t.subSkip),
                          )
                        : null,
                  ),
              ],
            ),
    );
  }

  String _visitState(T t, String state) => switch (state) {
    'booked' => t.subStateBooked,
    'skipped' => t.subStateSkipped,
    'missed' => t.subStateMissed,
    _ => t.subStatePlanned,
  };
}
