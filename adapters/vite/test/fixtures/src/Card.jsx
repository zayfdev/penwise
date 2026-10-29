import Widget from './Widget.jsx';

export function Card({ title }) {
  return (
    <section className="card">
      <h2>{title}</h2>
      <Widget />
      <my-el />
      <img src={title} />
    </section>
  );
}
