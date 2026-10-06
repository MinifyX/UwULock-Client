/** Which component draws which page (nav.ts's routes). */

import type { ReactNode } from 'react';
import type { Route } from '../nav';
import { CheckPage, EmailsPage, FindingsPage, ReviewPage } from './Check';
import { GeneratorPage } from './Generator';
import { ItemPage } from './Item';
import { ItemListPage } from './ItemList';
import { MaskedPage } from './Masked';
import { OverviewPage } from './Overview';
import { RequestPage, RequestsPage } from './Requests';
import { SendPage, SendsPage } from './Sends';
import { SettingsPage, SettingsSubPage } from './Settings';
import { SuitePage } from './Suite';

export function renderPage(route: Route): ReactNode {
  switch (route.page) {
    case 'overview':
      return <OverviewPage />;
    case 'list':
      return <ItemListPage filter={route.filter} />;
    case 'item':
      return <ItemPage id={route.id} />;
    case 'sends':
      return <SendsPage />;
    case 'send':
      return <SendPage id={route.id} />;
    case 'requests':
      return <RequestsPage />;
    case 'request':
      return <RequestPage id={route.id} />;
    case 'masked':
      return <MaskedPage />;
    case 'suite':
      return <SuitePage space={route.space} />;
    case 'check':
      return <CheckPage />;
    case 'findings':
      return <FindingsPage group={route.group} />;
    case 'review':
      return <ReviewPage />;
    case 'emails':
      return <EmailsPage />;
    case 'generator':
      return <GeneratorPage />;
    case 'settings':
      return <SettingsPage />;
    case 'settings-page':
      return <SettingsSubPage section={route.section} />;
  }
}
