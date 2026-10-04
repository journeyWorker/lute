Add a nested branch under the existing successful battle result for a declared host result: when `scene.battle.fight.fainted` is positive, report that the victory came at a cost. Keep the existing success line, assertion, and failure arm byte-identical. The bridge declares `won` with a default and `fainted` as an integer, so do not invent an unset or unknown result field; retain the scene identity and host contract.

Korean task name: 호스트 결과 분기 추가
