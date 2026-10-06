/**
 * UwUSSH's or UwURDP's records (UwU-Apps). The desktop's section, drawn in
 * its phone layout inside a page: its list, a record over it, its editors.
 */

import { useRef, useState } from 'react';
import { t, useLanguage } from '../../lib/i18n';
import type { SuiteSpace } from '../../lib/suiteModel';
import { SPACE_TITLE, SuitePane } from '../../components/SuitePane';
import { useMobile, useNav } from '../state';
import { BackLayer, Page } from '../ui';

export function SuitePage({ space }: { space: SuiteSpace }) {
  useLanguage();
  const { ipad, status } = useMobile();
  const nav = useNav();
  const [detail, setDetail] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  return (
    <Page title={t(SPACE_TITLE[space])}>
      <BackLayer open={detail} close={() => setDetail(false)} />
      <div className="m-embed">
        <div className="vault" data-pane={detail ? 'detail' : 'list'} data-embedded="">
          <SuitePane
            key={`${space}-${status.accountId}`}
            space={space}
            phone={!ipad}
            detailOpen={detail}
            onDetail={setDetail}
            onMenu={() => (nav.canBack ? nav.back() : undefined)}
            searchRef={searchRef}
          />
        </div>
      </div>
    </Page>
  );
}
