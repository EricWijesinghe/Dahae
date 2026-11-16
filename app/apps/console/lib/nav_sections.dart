// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

import 'package:core/core.dart';
import 'package:flutter/material.dart';
import 'package:ui/ui.dart';

import 'screens/audit.dart';
import 'screens/bookings.dart';
import 'screens/catalog.dart';
import 'screens/dashboard.dart';
import 'screens/finance.dart';
import 'screens/inbox.dart';
import 'screens/marketing.dart';
import 'screens/notify_templates.dart';
import 'screens/ops.dart';
import 'screens/owner_account.dart';
import 'screens/people.dart';
import 'screens/privacy_requests.dart';
import 'screens/renewals.dart';
import 'screens/rules.dart';
import 'screens/setup.dart';
import 'screens/subscriptions.dart';

/// One sidebar row. [id] is the Icon3DRegistry key: the rail draws that art
/// and the palette reuses its glyph, so a row and its icon cannot drift apart.
class ConsoleSection {
  const ConsoleSection(
    this.id,
    this.label,
    this.build, {
    this.adminOnly = false,
  });

  final String id;
  final String Function(T) label;
  final Widget Function() build;
  final bool adminOnly;

  IconData get icon =>
      (Icon3DRegistry.specs[id] ?? Icon3DRegistry.unknown).icon;
}

final consoleSections = <ConsoleSection>[
  ConsoleSection(
    'c_dashboard',
    (t) => t.cDashboard,
    () => const DashboardScreen(),
  ),
  ConsoleSection(
    'c_bookings',
    (t) => t.cBookings,
    () => const BookingsScreen(),
  ),
  ConsoleSection(
    'c_quotes',
    (t) => t.cQuoteDesk,
    () => const BookingsScreen(quoteDesk: true),
  ),
  ConsoleSection('c_chat', (t) => t.cInbox, () => const InboxScreen()),
  ConsoleSection('c_pricing', (t) => t.cCatalog, () => const CatalogScreen()),
  ConsoleSection(
    'c_marketing',
    (t) => t.cMarketing,
    () => const MarketingScreen(),
  ),
  ConsoleSection(
    'c_partners',
    (t) => t.cPartners,
    () => const PartnersScreen(),
  ),
  ConsoleSection(
    'c_payments',
    (t) => t.cSettlements,
    () => const SettlementsScreen(),
  ),
  ConsoleSection(
    'c_payouts',
    (t) => t.fFinance,
    () => const FinanceScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_subscriptions',
    (t) => t.cSubsTitle,
    () => const SubscriptionsScreen(),
  ),
  ConsoleSection(
    'c_customers',
    (t) => t.cCustomers,
    () => const CustomersScreen(),
  ),
  ConsoleSection(
    'c_staff',
    (t) => t.cStaff,
    () => const StaffScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_owner_account',
    (t) => t.oaTitle,
    () => const OwnerAccountScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_settings',
    (t) => t.cSetup,
    () => const SetupScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_rules',
    (t) => t.rulesTab,
    () => const RulesScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_notify',
    (t) => t.ntNavLabel,
    () => const NotifyTemplatesScreen(),
    adminOnly: true,
  ),
  ConsoleSection('c_monitor', (t) => t.cOps, () => const OpsScreen()),
  ConsoleSection(
    'c_audit',
    (t) => t.cAudit,
    () => const AuditScreen(),
    adminOnly: true,
  ),
  ConsoleSection(
    'c_privacy',
    (t) => t.prqTitle,
    () => const PrivacyRequestsScreen(),
  ),
  ConsoleSection(
    'c_renewals',
    (t) => t.rnwTitle,
    () => const RenewalsScreen(),
    adminOnly: true,
  ),
];
