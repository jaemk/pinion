begin;

create sequence pin.id_seq;
create or replace function pin.id_gen(out result bigint) as
$$
declare
    epoch_millis bigint := 1665829849339;
    seq_id       bigint;
    now_millis   bigint;
begin
    -- 1048576 comes from 2**20, see comment below
    select nextval('pin.id_seq') % 1048576 into seq_id;
    select floor(extract(epoch from clock_timestamp()) * 1000) into now_millis;
    -- we're starting with a bigint so 64 bits
    -- shifting over 20 bits uses the lower 44 bits of our millis timestamp
    -- 44 bits of millis is ~550 years
    result := (now_millis - epoch_millis) << 20;
    -- use the remaining 20 bits to store an identifier
    -- that's unique to this millisecond. That's where the
    -- 1048576 comes from (2**20) for calculating seq_id.
    -- the result is that we can generate 104876 unique 64 bit
    -- integers every millisecond for the next 550 years.
    result := result | (seq_id);
end;
$$ language plpgsql;

commit;
