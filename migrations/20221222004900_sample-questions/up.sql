begin;

do
$$
    declare
        i record;
    begin
        for i in 1..50 loop
                with qid as (
                    insert into
                        pin.questions (kind, prompt)
                        values ('multi', '5 poops a day or one poop every 5 days')
                        returning id
                )
                insert into
                    pin.question_multi_options (question_id, rank, value)
                values
                    ((select id from qid), 0, '5 poops a day'),
                    ((select id from qid), 1, 'One poop every 5 days');
            end loop;
    end;
$$
;

commit;